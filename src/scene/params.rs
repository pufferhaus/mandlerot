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
