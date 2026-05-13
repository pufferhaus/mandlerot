use super::meta::{ParamDef, SceneMeta};

/// Slot range used by both scenes and post-FX passes. Mirrors the
/// `seen = [false; 9]` check in `SceneMeta::validate` — extending the
/// slot count means bumping both sides.
const SLOTS: usize = 9;

/// Live param state. The earlier implementation stored values in a
/// `BTreeMap<String, f32>` keyed by param name, but every per-frame draw
/// looks values up *by slot*, not by name. We now keep values in a flat
/// `[f32; 9]` indexed directly by `ParamDef::slot`, which drops 18
/// BTreeMap walks per render at the cost of a one-time slot-to-name
/// resolution inside `get` / `set`.
///
/// Unused slots (scenes that declare fewer than 9 params) read as 0.0 —
/// same observable behaviour as the old code.
#[derive(Debug, Clone, Default)]
pub struct ParamMap {
    defs: Vec<ParamDef>,
    values: [f32; SLOTS],
    /// True when the value at `values[i]` was actually written (either
    /// from a scene default at construction or via `set`). Lets `get` and
    /// `slot_values` distinguish "param not declared" from "param declared,
    /// value 0".
    present: [bool; SLOTS],
}

impl ParamMap {
    pub fn from_scene(meta: &SceneMeta) -> Self {
        let defs = meta.params.clone();
        let mut values = [0.0; SLOTS];
        let mut present = [false; SLOTS];
        for p in &defs {
            let slot = p.slot as usize;
            if slot < SLOTS {
                values[slot] = p.default;
                present[slot] = true;
            }
        }
        Self {
            defs,
            values,
            present,
        }
    }

    pub fn get(&self, name: &str) -> Option<f32> {
        let def = self.defs.iter().find(|p| p.name == name)?;
        let slot = def.slot as usize;
        if slot >= SLOTS || !self.present[slot] {
            return None;
        }
        Some(self.values[slot])
    }

    pub fn set(&mut self, name: &str, v: f32) -> bool {
        let Some(def) = self.defs.iter().find(|p| p.name == name) else {
            return false;
        };
        let slot = def.slot as usize;
        if slot >= SLOTS {
            return false;
        }
        self.values[slot] = v.clamp(def.min, def.max);
        self.present[slot] = true;
        true
    }

    pub fn reset_to_defaults(&mut self) {
        for d in &self.defs {
            let slot = d.slot as usize;
            if slot < SLOTS {
                self.values[slot] = d.default;
                self.present[slot] = true;
            }
        }
    }

    /// Override a param's audio route at runtime. Returns the new route so
    /// the caller can surface it in the status panel / last-action label.
    /// When transitioning from `AudioRoute::None` to any band, seed a
    /// default `audio_amount` of 0.5 (full positive polarity) so the param
    /// becomes audibly reactive — otherwise the assignment would be inert.
    pub fn set_audio_route(&mut self, name: &str, route: crate::scene::AudioRoute) -> Option<crate::scene::AudioRoute> {
        let def = self.defs.iter_mut().find(|d| d.name == name)?;
        let was_none = matches!(def.audio_route, crate::scene::AudioRoute::None);
        def.audio_route = route;
        if !matches!(route, crate::scene::AudioRoute::None) && was_none && def.audio_amount == 0.0 {
            def.audio_amount = 0.5;
            def.audio_polarity = 1.0;
        }
        Some(route)
    }

    /// Cycle the audio route of `name` by `dir` (±1) through the canonical
    /// sequence: None → Bass → Lomid → Mid → Himid → Treble → Beat → None.
    /// Mid sits between Lomid and Himid in the cycle to mirror its
    /// frequency-axis position.
    pub fn cycle_audio_route(&mut self, name: &str, dir: i8) -> Option<crate::scene::AudioRoute> {
        use crate::scene::AudioRoute;
        let order = [
            AudioRoute::None,
            AudioRoute::Bass,
            AudioRoute::Lomid,
            AudioRoute::Mid,
            AudioRoute::Himid,
            AudioRoute::Treble,
            AudioRoute::Beat,
        ];
        let def = self.defs.iter().find(|d| d.name == name)?;
        let idx = order.iter().position(|r| *r == def.audio_route).unwrap_or(0) as i32;
        let len = order.len() as i32;
        let next = order[((idx + dir as i32).rem_euclid(len)) as usize];
        self.set_audio_route(name, next)
    }

    /// Return values indexed by slot (0..9). Missing slots default to 0.0.
    /// Direct copy of the internal array — no per-slot lookup at all.
    pub fn slot_values(&self) -> [f32; SLOTS] {
        self.values
    }

    /// Compute `(slot → effective value)` taking audio routing and bypass
    /// into account. `bands[0..5]` = [bass, lomid, himid, treble, mid].
    /// Single pass over the (small) defs vec; no map walks.
    pub fn effective_slot_values(&self, bands: &[f32; 5], bypass: bool) -> [f32; SLOTS] {
        let mut out = self.values;
        if bypass {
            return out;
        }
        for d in &self.defs {
            let slot = d.slot as usize;
            if slot >= SLOTS || !self.present[slot] {
                continue;
            }
            let band_value = match d.audio_route {
                crate::scene::AudioRoute::None | crate::scene::AudioRoute::Beat => 0.0,
                crate::scene::AudioRoute::Bass => bands[0],
                crate::scene::AudioRoute::Lomid => bands[1],
                crate::scene::AudioRoute::Himid => bands[2],
                crate::scene::AudioRoute::Treble => bands[3],
                crate::scene::AudioRoute::Mid => bands[4],
            };
            let extra = d.audio_amount * d.audio_polarity * band_value;
            out[slot] = (self.values[slot] + extra * (d.max - d.min)).clamp(d.min, d.max);
        }
        out
    }

    pub fn defs(&self) -> &[ParamDef] {
        &self.defs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::meta::SceneMeta;

    fn meta() -> SceneMeta {
        SceneMeta::parse(include_str!("../../tests/fixtures/good_scene.toml"), "good").unwrap()
    }

    #[test]
    fn from_scene_uses_defaults() {
        let m = meta();
        let p = ParamMap::from_scene(&m);
        assert_eq!(p.get("zoom"), Some(1.0));
        assert_eq!(p.get("hue"), Some(0.5));
    }

    #[test]
    fn set_clamps_to_range() {
        let m = meta();
        let mut p = ParamMap::from_scene(&m);
        assert!(p.set("zoom", 100.0));
        assert_eq!(p.get("zoom"), Some(10.0)); // clamped to max
        assert!(p.set("zoom", -5.0));
        assert_eq!(p.get("zoom"), Some(0.1)); // clamped to min
    }

    #[test]
    fn set_rejects_unknown_name() {
        let m = meta();
        let mut p = ParamMap::from_scene(&m);
        assert!(!p.set("nonexistent", 1.0));
    }

    #[test]
    fn slot_values_indexes_by_slot() {
        let m = meta();
        let p = ParamMap::from_scene(&m);
        let s = p.slot_values();
        assert_eq!(s[0], 1.0); // zoom is slot 0
        assert_eq!(s[1], 0.5); // hue is slot 1
        assert_eq!(s[2], 0.0); // unset slot
    }

    #[test]
    fn reset_restores_defaults() {
        let m = meta();
        let mut p = ParamMap::from_scene(&m);
        p.set("zoom", 5.0);
        p.reset_to_defaults();
        assert_eq!(p.get("zoom"), Some(1.0));
    }
}

#[cfg(test)]
mod audio_route_tests {
    use super::*;
    use crate::scene::SceneMeta;

    fn meta_with_route(name: &str, route: &str, amount: f32) -> SceneMeta {
        let s = format!(
            r#"
                name = "x"
                [[params]]
                slot = 0
                name = "{name}"
                min = 0.0
                max = 1.0
                default = 0.5
                audio_route = "{route}"
                audio_amount = {amount}
            "#
        );
        SceneMeta::parse(&s, "inline").unwrap()
    }

    #[test]
    fn bass_route_adds_audio_to_base() {
        let m = meta_with_route("zoom", "bass", 0.5);
        let p = ParamMap::from_scene(&m);
        let bands = [0.4, 0.0, 0.0, 0.0, 0.0];
        let slots = p.effective_slot_values(&bands, false);
        // base 0.5 + 0.5 * 0.4 * 1.0 (range) = 0.7
        assert!((slots[0] - 0.7).abs() < 1e-5);
    }

    #[test]
    fn bypass_returns_base_only() {
        let m = meta_with_route("zoom", "treble", 0.5);
        let p = ParamMap::from_scene(&m);
        let bands = [0.0, 0.0, 0.0, 0.9, 0.0];
        let slots = p.effective_slot_values(&bands, true);
        assert!((slots[0] - 0.5).abs() < 1e-5);
    }
}
