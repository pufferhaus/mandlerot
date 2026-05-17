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
    /// Optional per-Look post-FX snapshot. Missing == "never bound, don't
    /// touch the chain on recall". See `PostFxSnapshot` for the tri-state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postfx: Option<PostFxSnapshot>,
}

/// A captured post-FX chain attached to a Look slot. `active=false` means
/// the snapshot is preserved on disk but skipped on recall (paused).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PostFxSnapshot {
    pub active: bool,
    pub passes: Vec<PostFxPassSnapshot>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PostFxPassSnapshot {
    pub name: String,
    pub enabled: bool,
    pub params: BTreeMap<String, f32>,
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
    /// Preserves any existing `postfx` snapshot on this slot (auto-sync
    /// owns that field, not scene/params save).
    pub fn save(&mut self, slot: u8, state: &SharedState, name: Option<String>) -> Result<()> {
        let key = slot.to_string();
        let prior_postfx = self.file.slots.get(&key).and_then(|l| l.postfx.clone());
        let look = Look {
            name: name.unwrap_or_else(crate::preset::names::random_look_name),
            saved_at: now_iso8601(),
            scene_a: state.layer_a.scene_name.clone(),
            scene_b: state.layer_b.scene_name.clone(),
            xfade: state.xfade,
            blend_mode: blend_mode_str(state.blend_mode).to_string(),
            audio_bypass: state.audio_bypass,
            params_a: param_values(&state.layer_a.params),
            params_b: param_values(&state.layer_b.params),
            postfx: prior_postfx,
        };
        self.file.slots.insert(key, look);
        self.file.version = 2;
        self.flush()
    }

    /// Recall slot N. Mutates `state` to match. Missing scenes log a warning
    /// and leave that layer alone. If the slot owns a postfx snapshot with
    /// `active=true`, invokes `apply_postfx` with the snapshot — the caller
    /// is responsible for forwarding it to the live `PostFx`. Decoupled this
    /// way so the caller controls the `PostFx` borrow and so tests don't
    /// need a GL context.
    pub fn recall(
        &self,
        slot: u8,
        state: &mut SharedState,
        lib: &SceneLibrary,
        mut apply_postfx: impl FnMut(&PostFxSnapshot),
    ) -> Result<()> {
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
        if let Some(snap) = p.postfx.as_ref().filter(|s| s.active) {
            apply_postfx(snap);
        }
        Ok(())
    }

    /// True when slot has a snapshot AND active=true.
    pub fn is_bound_active(&self, slot: u8) -> bool {
        self.file
            .slots
            .get(&slot.to_string())
            .and_then(|l| l.postfx.as_ref())
            .map(|s| s.active)
            .unwrap_or(false)
    }

    /// True when slot has any snapshot (active or paused).
    pub fn has_snapshot(&self, slot: u8) -> bool {
        self.file
            .slots
            .get(&slot.to_string())
            .and_then(|l| l.postfx.as_ref())
            .is_some()
    }

    /// Write a snapshot onto slot N. Errors if the slot has no Look (i.e.
    /// nothing to bind to). Flushes atomically.
    pub fn save_postfx_snapshot(&mut self, slot: u8, snap: PostFxSnapshot) -> Result<()> {
        let key = slot.to_string();
        let Some(look) = self.file.slots.get_mut(&key) else {
            return Err(Error::Backend(format!(
                "postfx bind: slot {slot} has no saved Look"
            )));
        };
        look.postfx = Some(snap);
        self.file.version = 2;
        self.flush()
    }

    /// Flip the `active` flag on an existing snapshot without touching passes.
    /// Errors if the slot has no snapshot.
    pub fn set_postfx_active(&mut self, slot: u8, active: bool) -> Result<()> {
        let key = slot.to_string();
        let look = self.file.slots.get_mut(&key).ok_or_else(|| {
            Error::Backend(format!("postfx active: slot {slot} has no saved Look"))
        })?;
        let snap = look.postfx.as_mut().ok_or_else(|| {
            Error::Backend(format!("postfx active: slot {slot} has no snapshot"))
        })?;
        snap.active = active;
        self.file.version = 2;
        self.flush()
    }

    /// Call after any post-FX chain mutation (toggle, param nudge, reset,
    /// hot-reload). If `active_slot` names a Look that is bound+active,
    /// overwrites that slot's snapshot with `snap`. Otherwise a no-op.
    ///
    /// Takes the snapshot by value so this module stays decoupled from
    /// `render::postfx`. Callers do `pfx.snapshot()` to produce the value.
    pub fn after_postfx_mutation(
        &mut self,
        active_slot: Option<u8>,
        snap: PostFxSnapshot,
    ) -> Result<()> {
        let Some(slot) = active_slot else { return Ok(()) };
        if !self.is_bound_active(slot) {
            return Ok(());
        }
        self.save_postfx_snapshot(slot, snap)
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
        BlendMode::Overlay => "overlay",
        BlendMode::HardLight => "hardlight",
        BlendMode::Lighten => "lighten",
        BlendMode::Darken => "darken",
        BlendMode::Exclusion => "exclusion",
        BlendMode::Subtract => "subtract",
        BlendMode::LinearBurn => "linearburn",
        BlendMode::SoftLight => "softlight",
        BlendMode::ColorDodge => "colordodge",
        BlendMode::ColorBurn => "colorburn",
        BlendMode::Hue => "hue",
        BlendMode::Saturation => "saturation",
        BlendMode::Color => "color",
        BlendMode::Luminosity => "luminosity",
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

        store.recall(3, &mut s, &lib, |_| {}).unwrap();
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
        let res = store.recall(1, &mut s, &limited, |_| {});
        assert!(
            res.is_ok(),
            "recall should warn, not error, on missing scene"
        );
    }

    #[test]
    fn recall_with_active_snapshot_invokes_callback() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.json");
        let lib = library();
        let s = state(&lib);
        let mut store = LookStore::load_or_empty(&path).unwrap();
        store.save(1, &s, None).unwrap();
        let snap = PostFxSnapshot {
            active: true,
            passes: vec![PostFxPassSnapshot {
                name: "vignette".into(),
                enabled: true,
                params: BTreeMap::from([("amount".to_string(), 0.9f32)]),
            }],
        };
        store.save_postfx_snapshot(1, snap).unwrap();

        let mut received: Option<PostFxSnapshot> = None;
        let mut s2 = state(&lib);
        store
            .recall(1, &mut s2, &lib, |snap| received = Some(snap.clone()))
            .unwrap();
        let got = received.expect("callback should fire for active snapshot");
        assert!(got.active);
        assert_eq!(got.passes[0].name, "vignette");
    }

    #[test]
    fn recall_with_paused_snapshot_skips_callback() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.json");
        let lib = library();
        let s = state(&lib);
        let mut store = LookStore::load_or_empty(&path).unwrap();
        store.save(1, &s, None).unwrap();
        let snap = PostFxSnapshot {
            active: false,
            passes: vec![PostFxPassSnapshot {
                name: "vignette".into(),
                enabled: true,
                params: BTreeMap::new(),
            }],
        };
        store.save_postfx_snapshot(1, snap).unwrap();

        let mut fired = false;
        let mut s2 = state(&lib);
        store
            .recall(1, &mut s2, &lib, |_| fired = true)
            .unwrap();
        assert!(!fired, "paused snapshot must not invoke the callback");
    }

    #[test]
    fn recall_with_no_snapshot_skips_callback() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.json");
        let lib = library();
        let s = state(&lib);
        let mut store = LookStore::load_or_empty(&path).unwrap();
        store.save(1, &s, None).unwrap(); // no snapshot ever bound

        let mut fired = false;
        let mut s2 = state(&lib);
        store
            .recall(1, &mut s2, &lib, |_| fired = true)
            .unwrap();
        assert!(!fired, "no-snapshot slot must not invoke the callback");
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
    fn v1_fixture_loads_with_no_postfx() {
        let s = include_str!("../../tests/fixtures/preset_v1.json");
        let f: LooksFile = serde_json::from_str(s).unwrap();
        assert_eq!(f.version, 1);
        let p = f.slots.get("1").unwrap();
        assert!(p.postfx.is_none(), "v1 slot must load with postfx=None");
    }

    #[test]
    fn v2_fixture_round_trips_with_postfx() {
        let s = include_str!("../../tests/fixtures/preset_v2_with_postfx.json");
        let f: LooksFile = serde_json::from_str(s).unwrap();
        assert_eq!(f.version, 2);

        let bound = f.slots.get("1").unwrap();
        let snap = bound.postfx.as_ref().expect("slot 1 has snapshot");
        assert!(snap.active);
        assert_eq!(snap.passes.len(), 2);
        assert_eq!(snap.passes[0].name, "bloom_hq");
        assert!(snap.passes[0].enabled);
        assert_eq!(snap.passes[0].params.get("strength"), Some(&0.6));

        let paused = f.slots.get("2").unwrap();
        let snap = paused.postfx.as_ref().expect("slot 2 has snapshot");
        assert!(!snap.active);
        assert_eq!(snap.passes.len(), 1);
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
    fn save_preserves_existing_postfx_snapshot() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.json");
        let lib = library();
        let s = state(&lib);
        let mut store = LookStore::load_or_empty(&path).unwrap();
        // Initial save (no postfx yet)
        store.save(1, &s, Some("first".into())).unwrap();
        // Inject a snapshot directly (simulating what the bind-toggle path will do)
        let snap = PostFxSnapshot {
            active: true,
            passes: vec![PostFxPassSnapshot {
                name: "vignette".into(),
                enabled: true,
                params: BTreeMap::from([("amount".to_string(), 0.4f32)]),
            }],
        };
        store.file.slots.get_mut("1").unwrap().postfx = Some(snap);
        store.flush().unwrap();
        // Re-save same slot (e.g. user changed scene params) — postfx must survive
        store.save(1, &s, Some("second".into())).unwrap();
        let reloaded = LookStore::load_or_empty(&path).unwrap();
        let p = reloaded.file.slots.get("1").unwrap();
        assert_eq!(p.name, "second");
        let snap = p.postfx.as_ref().expect("postfx preserved");
        assert!(snap.active);
        assert_eq!(snap.passes[0].name, "vignette");
        assert_eq!(reloaded.file.version, 2);
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

    #[test]
    fn is_bound_active_three_states() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.json");
        let lib = library();
        let s = state(&lib);
        let mut store = LookStore::load_or_empty(&path).unwrap();
        store.save(1, &s, None).unwrap();
        // (a) no snapshot
        assert!(!store.is_bound_active(1));
        assert!(!store.has_snapshot(1));
        // (b) snapshot, paused
        let snap = PostFxSnapshot {
            active: false,
            passes: vec![],
        };
        store.save_postfx_snapshot(1, snap).unwrap();
        store.set_postfx_active(1, false).unwrap();
        assert!(store.has_snapshot(1));
        assert!(!store.is_bound_active(1));
        // (c) snapshot, active
        store.set_postfx_active(1, true).unwrap();
        assert!(store.is_bound_active(1));
    }

    #[test]
    fn save_postfx_snapshot_on_empty_slot_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.json");
        let mut store = LookStore::load_or_empty(&path).unwrap();
        let snap = PostFxSnapshot { active: true, passes: vec![] };
        let err = store.save_postfx_snapshot(1, snap);
        assert!(err.is_err(), "binding postfx to empty slot must error");
    }

    #[test]
    fn save_postfx_snapshot_persists_atomically() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.json");
        let lib = library();
        let s = state(&lib);
        let mut store = LookStore::load_or_empty(&path).unwrap();
        store.save(1, &s, None).unwrap();
        let snap = PostFxSnapshot {
            active: true,
            passes: vec![PostFxPassSnapshot {
                name: "grain".into(),
                enabled: true,
                params: BTreeMap::from([("amount".to_string(), 0.25f32)]),
            }],
        };
        store.save_postfx_snapshot(1, snap).unwrap();
        let reloaded = LookStore::load_or_empty(&path).unwrap();
        let p = reloaded.file.slots.get("1").unwrap();
        let snap = p.postfx.as_ref().unwrap();
        assert_eq!(snap.passes[0].params.get("amount"), Some(&0.25));
    }

    #[test]
    fn set_postfx_active_on_no_snapshot_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.json");
        let lib = library();
        let s = state(&lib);
        let mut store = LookStore::load_or_empty(&path).unwrap();
        store.save(1, &s, None).unwrap();
        let err = store.set_postfx_active(1, true);
        assert!(err.is_err(), "set_postfx_active without snapshot must error");
    }

    fn snap_with(name: &str, enabled: bool, amount: f32, active: bool) -> PostFxSnapshot {
        PostFxSnapshot {
            active,
            passes: vec![PostFxPassSnapshot {
                name: name.into(),
                enabled,
                params: BTreeMap::from([("amount".to_string(), amount)]),
            }],
        }
    }

    #[test]
    fn after_mutation_writes_snapshot_when_bound_active() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.json");
        let lib = library();
        let s = state(&lib);
        let mut store = LookStore::load_or_empty(&path).unwrap();
        store.save(1, &s, None).unwrap();
        store
            .save_postfx_snapshot(1, snap_with("vignette", false, 0.0, true))
            .unwrap();
        // Live chain produces a fresh snapshot with the user's edit.
        let live = snap_with("vignette", true, 0.8, true);
        store.after_postfx_mutation(Some(1), live).unwrap();
        let reloaded = LookStore::load_or_empty(&path).unwrap();
        let p = reloaded.file.slots.get("1").unwrap().postfx.as_ref().unwrap();
        assert!(p.passes[0].enabled);
        assert_eq!(p.passes[0].params.get("amount"), Some(&0.8));
    }

    #[test]
    fn after_mutation_noop_when_no_active_look() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.json");
        let mut store = LookStore::load_or_empty(&path).unwrap();
        let live = snap_with("vignette", true, 0.5, true);
        store.after_postfx_mutation(None, live).unwrap();
        assert!(store.file.slots.is_empty());
    }

    #[test]
    fn after_mutation_noop_when_paused() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.json");
        let lib = library();
        let s = state(&lib);
        let mut store = LookStore::load_or_empty(&path).unwrap();
        store.save(1, &s, None).unwrap();
        store
            .save_postfx_snapshot(1, snap_with("vignette", false, 0.0, false))
            .unwrap();
        let live = snap_with("vignette", true, 0.5, true);
        store.after_postfx_mutation(Some(1), live).unwrap();
        let reloaded = LookStore::load_or_empty(&path).unwrap();
        let p = reloaded.file.slots.get("1").unwrap().postfx.as_ref().unwrap();
        assert!(!p.passes[0].enabled);
        assert_eq!(p.passes[0].params.get("amount"), Some(&0.0));
    }

    #[test]
    fn after_mutation_noop_when_no_snapshot() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.json");
        let lib = library();
        let s = state(&lib);
        let mut store = LookStore::load_or_empty(&path).unwrap();
        store.save(1, &s, None).unwrap();
        let live = snap_with("vignette", true, 0.5, true);
        store.after_postfx_mutation(Some(1), live).unwrap();
        assert!(store.file.slots.get("1").unwrap().postfx.is_none());
    }

    #[test]
    fn save_with_no_name_uses_random_handle() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.json");
        let lib = library();
        let s = state(&lib);
        let mut store = LookStore::load_or_empty(&path).unwrap();
        store.save(1, &s, None).unwrap();
        let look = store.file.slots.get("1").unwrap();
        assert_ne!(look.name, "slot 1");
        let parts: Vec<&str> = look.name.split('-').collect();
        assert_eq!(parts.len(), 2, "name was: {}", look.name);
    }
}
