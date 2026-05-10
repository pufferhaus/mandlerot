use std::collections::BTreeMap;

use super::meta::{ParamDef, SceneMeta};

/// Live map of param name → current f32 value, with the scene's defs for clamp/curve.
#[derive(Debug, Clone)]
pub struct ParamMap {
    defs: Vec<ParamDef>,
    values: BTreeMap<String, f32>,
}

impl Default for ParamMap {
    fn default() -> Self {
        Self {
            defs: Vec::new(),
            values: std::collections::BTreeMap::new(),
        }
    }
}

impl ParamMap {
    pub fn from_scene(meta: &SceneMeta) -> Self {
        let defs = meta.params.clone();
        let values = defs.iter().map(|p| (p.name.clone(), p.default)).collect();
        Self { defs, values }
    }

    pub fn get(&self, name: &str) -> Option<f32> {
        self.values.get(name).copied()
    }

    pub fn set(&mut self, name: &str, v: f32) -> bool {
        let Some(def) = self.defs.iter().find(|p| p.name == name) else {
            return false;
        };
        let clamped = v.clamp(def.min, def.max);
        self.values.insert(name.to_string(), clamped);
        true
    }

    pub fn reset_to_defaults(&mut self) {
        for d in &self.defs {
            self.values.insert(d.name.clone(), d.default);
        }
    }

    /// Return values indexed by slot (0..8). Missing slots default to 0.0.
    pub fn slot_values(&self) -> [f32; 8] {
        let mut out = [0.0; 8];
        for d in &self.defs {
            if let Some(v) = self.values.get(&d.name) {
                out[d.slot as usize] = *v;
            }
        }
        out
    }

    /// Compute `(slot → effective value)` taking audio routing and bypass
    /// into account. `bands[0..4]` = [bass, lomid, himid, treble].
    pub fn effective_slot_values(&self, bands: &[f32; 4], bypass: bool) -> [f32; 8] {
        let mut out = self.slot_values();
        if bypass {
            return out;
        }
        for d in &self.defs {
            if let Some(base) = self.values.get(&d.name) {
                let band_value = match d.audio_route {
                    crate::scene::AudioRoute::None | crate::scene::AudioRoute::Beat => 0.0,
                    crate::scene::AudioRoute::Bass => bands[0],
                    crate::scene::AudioRoute::Lomid => bands[1],
                    crate::scene::AudioRoute::Himid => bands[2],
                    crate::scene::AudioRoute::Treble => bands[3],
                };
                let extra = d.audio_amount * d.audio_polarity * band_value;
                let v = (*base + extra * (d.max - d.min)).clamp(d.min, d.max);
                out[d.slot as usize] = v;
            }
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
        let bands = [0.4, 0.0, 0.0, 0.0];
        let slots = p.effective_slot_values(&bands, false);
        // base 0.5 + 0.5 * 0.4 * 1.0 (range) = 0.7
        assert!((slots[0] - 0.7).abs() < 1e-5);
    }

    #[test]
    fn bypass_returns_base_only() {
        let m = meta_with_route("zoom", "treble", 0.5);
        let p = ParamMap::from_scene(&m);
        let bands = [0.0, 0.0, 0.0, 0.9];
        let slots = p.effective_slot_values(&bands, true);
        assert!((slots[0] - 0.5).abs() < 1e-5);
    }
}
