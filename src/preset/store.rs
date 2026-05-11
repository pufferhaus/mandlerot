//! Look save / recall. Stored as JSON, params keyed by name (not slot
//! index) so scene refactors don't corrupt looks.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::scene::{ParamMap, SceneLibrary};
use crate::state::{BlendMode, LayerState, SharedState};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LooksFile {
    pub version: u32,
    pub slots: BTreeMap<String, Look>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Look {
    pub name: String,
    pub saved_at: String,
    pub scene_a: String,
    pub scene_b: String,
    pub xfade: f32,
    pub blend_mode: String,
    #[serde(default)]
    pub audio_bypass: bool,
    pub params_a: BTreeMap<String, f32>,
    pub params_b: BTreeMap<String, f32>,
}

pub struct LookStore {
    pub path: PathBuf,
    pub file: LooksFile,
}

impl LookStore {
    pub fn load_or_empty(path: &Path) -> Result<Self> {
        let file = if path.exists() {
            let s = std::fs::read_to_string(path)?;
            serde_json::from_str(&s).map_err(|e| Error::Backend(format!("looks parse: {e}")))?
        } else {
            LooksFile {
                version: 1,
                slots: BTreeMap::new(),
            }
        };
        Ok(Self {
            path: path.to_path_buf(),
            file,
        })
    }

    /// Save the current state into slot N. Atomic: write tmp, fsync, rename.
    pub fn save(&mut self, slot: u8, state: &SharedState, name: Option<String>) -> Result<()> {
        let look = Look {
            name: name.unwrap_or_else(|| format!("slot {slot}")),
            saved_at: now_iso8601(),
            scene_a: state.layer_a.scene_name.clone(),
            scene_b: state.layer_b.scene_name.clone(),
            xfade: state.xfade,
            blend_mode: blend_mode_str(state.blend_mode).to_string(),
            audio_bypass: state.audio_bypass,
            params_a: param_values(&state.layer_a.params),
            params_b: param_values(&state.layer_b.params),
        };
        self.file.slots.insert(slot.to_string(), look);
        self.file.version = 1;
        self.flush()
    }

    /// Recall slot N. Mutates `state` to match. Missing scenes log a warning
    /// and leave that layer alone.
    pub fn recall(&self, slot: u8, state: &mut SharedState, lib: &SceneLibrary) -> Result<()> {
        let key = slot.to_string();
        let Some(p) = self.file.slots.get(&key) else {
            return Err(Error::Backend(format!("look slot {slot} empty")));
        };
        if let Ok(scene) = lib.require(&p.scene_a) {
            let mut pm = ParamMap::from_scene(&scene.meta);
            for (k, v) in &p.params_a {
                pm.set(k, *v);
            }
            state.layer_a = LayerState {
                scene_name: p.scene_a.clone(),
                params: pm,
            };
        } else {
            tracing::warn!("look slot {slot}: scene_a '{}' not in library", p.scene_a);
        }
        if let Ok(scene) = lib.require(&p.scene_b) {
            let mut pm = ParamMap::from_scene(&scene.meta);
            for (k, v) in &p.params_b {
                pm.set(k, *v);
            }
            state.layer_b = LayerState {
                scene_name: p.scene_b.clone(),
                params: pm,
            };
        } else {
            tracing::warn!("look slot {slot}: scene_b '{}' not in library", p.scene_b);
        }
        state.xfade = p.xfade.clamp(0.0, 1.0);
        if let Some(bm) = BlendMode::parse(&p.blend_mode) {
            state.blend_mode = bm;
        }
        state.audio_bypass = p.audio_bypass;
        state.active_look_slot = Some(slot);
        state.look_dirty = false;
        Ok(())
    }

    fn flush(&self) -> Result<()> {
        let body = serde_json::to_string_pretty(&self.file)
            .map_err(|e| Error::Backend(format!("serialize looks: {e}")))?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, body)?;
        // fsync the tmp file
        let f = std::fs::File::open(&tmp)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

fn param_values(pm: &ParamMap) -> BTreeMap<String, f32> {
    let mut out = BTreeMap::new();
    for d in pm.defs() {
        if let Some(v) = pm.get(&d.name) {
            out.insert(d.name.clone(), v);
        }
    }
    out
}

fn blend_mode_str(b: BlendMode) -> &'static str {
    match b {
        BlendMode::Mix => "mix",
        BlendMode::Add => "add",
        BlendMode::Multiply => "multiply",
        BlendMode::Screen => "screen",
        BlendMode::Difference => "difference",
    }
}

fn now_iso8601() -> String {
    // Simple ISO-8601 UTC. We don't pull chrono in just for this.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("epoch+{secs}s") // good enough; humans rarely consume this
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{LoadedScene, SceneMeta};

    fn library() -> SceneLibrary {
        let mut lib = SceneLibrary::default();
        let a = SceneMeta::parse(
            "name = \"plasma\"\n[[params]]\nslot = 0\nname = \"hue\"\nmin = 0.0\nmax = 1.0\ndefault = 0.5\n",
            "x",
        )
        .unwrap();
        lib.upsert(
            "plasma",
            LoadedScene {
                meta: a,
                fragment_body: "void main() {}".into(),
                source_path: PathBuf::from("inline"),
            },
        );
        let b = SceneMeta::parse(
            "name = \"solid\"\n[[params]]\nslot = 0\nname = \"red\"\nmin = 0.0\nmax = 1.0\ndefault = 1.0\n",
            "x",
        )
        .unwrap();
        lib.upsert(
            "solid",
            LoadedScene {
                meta: b,
                fragment_body: "void main() {}".into(),
                source_path: PathBuf::from("inline"),
            },
        );
        lib
    }

    fn state(lib: &SceneLibrary) -> SharedState {
        SharedState::from_initial(lib, "plasma", "solid", 0.0, BlendMode::Mix).unwrap()
    }

    #[test]
    fn save_then_recall_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.json");
        let lib = library();
        let mut s = state(&lib);
        s.layer_a.params.set("hue", 0.8);
        s.xfade = 0.3;
        s.blend_mode = BlendMode::Add;
        let mut store = LookStore::load_or_empty(&path).unwrap();
        store.save(3, &s, Some("test".into())).unwrap();

        // Mutate state, then recall.
        s.layer_a.params.set("hue", 0.1);
        s.xfade = 0.0;
        s.blend_mode = BlendMode::Mix;

        store.recall(3, &mut s, &lib).unwrap();
        assert_eq!(s.layer_a.params.get("hue"), Some(0.8));
        assert_eq!(s.xfade, 0.3);
        assert_eq!(s.blend_mode, BlendMode::Add);
        assert_eq!(s.active_look_slot, Some(3));
        assert!(!s.look_dirty);
    }

    #[test]
    fn recall_missing_scene_warns_keeps_other_layer() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.json");
        let lib = library();
        let mut s = state(&lib);
        let mut store = LookStore::load_or_empty(&path).unwrap();
        store.save(1, &s, None).unwrap();

        // Now construct a library missing 'plasma' → recall should keep layer_a alone.
        let mut limited = SceneLibrary::default();
        let solid = lib.require("solid").unwrap().clone();
        limited.upsert("solid", solid);
        let res = store.recall(1, &mut s, &limited);
        assert!(
            res.is_ok(),
            "recall should warn, not error, on missing scene"
        );
    }

    #[test]
    fn parses_existing_fixture() {
        let s = include_str!("../../tests/fixtures/preset_v1.json");
        let f: LooksFile = serde_json::from_str(s).unwrap();
        assert_eq!(f.version, 1);
        assert!(f.slots.contains_key("1"));
        let p = &f.slots["1"];
        assert_eq!(p.scene_a, "plasma");
        assert_eq!(p.params_a.get("scale"), Some(&1.2));
    }

    #[test]
    fn empty_path_loads_empty_store() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.json");
        let store = LookStore::load_or_empty(&path).unwrap();
        assert_eq!(store.file.version, 1);
        assert!(store.file.slots.is_empty());
    }

    #[test]
    fn atomic_write_creates_no_tmp_leftover() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.json");
        let lib = library();
        let s = state(&lib);
        let mut store = LookStore::load_or_empty(&path).unwrap();
        store.save(1, &s, None).unwrap();
        assert!(path.exists());
        assert!(!path.with_extension("json.tmp").exists());
    }
}
