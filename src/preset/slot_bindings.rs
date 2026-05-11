//! Persistent map: slot number (1..9) -> scene name.
//!
//! When a slot has no binding, the legacy alphabetical-by-index resolution
//! is used so first-run experience is unchanged. Bindings let the user pin
//! preferred scenes to slot keys via the in-app menu.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const SLOT_COUNT: usize = 9;
pub const FILE_NAME: &str = "slots.toml";

/// On-disk representation: a sparse map from slot number to scene name.
/// `toml` can't serialize `Option::None`, so we omit empty slots entirely.
#[derive(Debug, Default, Serialize, Deserialize)]
struct SlotsFile {
    #[serde(default)]
    slots: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct SlotBindings {
    /// Index i (0-based) holds the scene name bound to slot key `i + 1`.
    /// `None` means "fall back to alphabetical Nth in the library".
    pub entries: Vec<Option<String>>,
}

impl Default for SlotBindings {
    fn default() -> Self {
        Self {
            entries: vec![None; SLOT_COUNT],
        }
    }
}

impl SlotBindings {
    fn to_file(&self) -> SlotsFile {
        let mut slots = BTreeMap::new();
        for (i, e) in self.entries.iter().enumerate() {
            if let Some(name) = e {
                slots.insert((i + 1).to_string(), name.clone());
            }
        }
        SlotsFile { slots }
    }

    fn from_file(f: SlotsFile) -> Self {
        let mut out = Self::default();
        for (k, v) in f.slots {
            if let Ok(n) = k.parse::<u8>() {
                out.set(n, Some(v));
            }
        }
        out
    }
}

impl SlotBindings {
    pub fn path_in(state_dir: &Path) -> PathBuf {
        state_dir.join(FILE_NAME)
    }

    /// Read bindings from `state_dir/slots.toml`. Missing file → empty.
    pub fn load_or_empty(state_dir: &Path) -> Self {
        let path = Self::path_in(state_dir);
        let Ok(s) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match toml::from_str::<SlotsFile>(&s) {
            Ok(f) => Self::from_file(f),
            Err(e) => {
                tracing::warn!("slots.toml parse: {e}; using empty bindings");
                Self::default()
            }
        }
    }

    /// Get binding for slot key `n` (1..=9). `None` = unbound.
    pub fn get(&self, n: u8) -> Option<&str> {
        if !(1..=SLOT_COUNT as u8).contains(&n) {
            return None;
        }
        self.entries
            .get(n as usize - 1)
            .and_then(|o| o.as_deref())
    }

    /// Set binding for slot key `n` (1..=9). Pass `None` to clear.
    pub fn set(&mut self, n: u8, scene: Option<String>) {
        if !(1..=SLOT_COUNT as u8).contains(&n) {
            return;
        }
        if self.entries.len() < SLOT_COUNT {
            self.entries.resize(SLOT_COUNT, None);
        }
        self.entries[n as usize - 1] = scene;
    }

    /// Atomic write to `state_dir/slots.toml`.
    pub fn save(&self, state_dir: &Path) -> Result<()> {
        let path = Self::path_in(state_dir);
        std::fs::create_dir_all(state_dir)?;
        let body = toml::to_string_pretty(&self.to_file())
            .map_err(|e| Error::Backend(format!("serialize slots: {e}")))?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, body)?;
        let f = std::fs::File::open(&tmp)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }
}

/// Resolve slot `n` (1..=9) to a scene name. Returns the explicit binding
/// if present, otherwise the alphabetical Nth scene from `lib_names`
/// (which the caller should pre-filter to skip `__safe__` etc.).
pub fn resolve_slot<'a>(
    bindings: &'a SlotBindings,
    lib_names: &'a [String],
    n: u8,
) -> Option<&'a str> {
    if let Some(name) = bindings.get(n) {
        // If the binding points at a scene that no longer exists, fall
        // through to the alphabetical fallback rather than break the slot.
        if lib_names.iter().any(|s| s == name) {
            return Some(name);
        }
    }
    let idx = n as usize - 1;
    lib_names.get(idx).map(|s| s.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_all_none() {
        let b = SlotBindings::default();
        assert_eq!(b.entries.len(), SLOT_COUNT);
        assert!(b.entries.iter().all(|e| e.is_none()));
    }

    #[test]
    fn set_get_roundtrip() {
        let mut b = SlotBindings::default();
        b.set(1, Some("mandelbrot".into()));
        b.set(9, Some("ascii_rain".into()));
        assert_eq!(b.get(1), Some("mandelbrot"));
        assert_eq!(b.get(9), Some("ascii_rain"));
        assert_eq!(b.get(5), None);
    }

    #[test]
    fn out_of_range_set_is_noop() {
        let mut b = SlotBindings::default();
        b.set(0, Some("x".into()));
        b.set(10, Some("y".into()));
        assert!(b.entries.iter().all(|e| e.is_none()));
    }

    #[test]
    fn save_then_load_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let mut b = SlotBindings::default();
        b.set(2, Some("plasma".into()));
        b.set(7, Some("juliabulb".into()));
        b.save(tmp.path()).unwrap();
        let b2 = SlotBindings::load_or_empty(tmp.path());
        assert_eq!(b2.get(2), Some("plasma"));
        assert_eq!(b2.get(7), Some("juliabulb"));
        assert_eq!(b2.get(1), None);
    }

    #[test]
    fn missing_file_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let b = SlotBindings::load_or_empty(tmp.path());
        assert!(b.entries.iter().all(|e| e.is_none()));
    }

    #[test]
    fn resolve_uses_binding_when_present() {
        let b = {
            let mut b = SlotBindings::default();
            b.set(1, Some("third".into()));
            b
        };
        let names = vec!["alpha".to_string(), "beta".into(), "third".into()];
        assert_eq!(resolve_slot(&b, &names, 1), Some("third"));
    }

    #[test]
    fn resolve_falls_back_to_alphabetical_when_unbound() {
        let b = SlotBindings::default();
        let names = vec!["alpha".to_string(), "beta".into(), "gamma".into()];
        assert_eq!(resolve_slot(&b, &names, 1), Some("alpha"));
        assert_eq!(resolve_slot(&b, &names, 3), Some("gamma"));
    }

    #[test]
    fn resolve_falls_back_when_binding_is_stale() {
        let mut b = SlotBindings::default();
        b.set(1, Some("deleted_scene".into()));
        let names = vec!["alpha".to_string(), "beta".into()];
        assert_eq!(resolve_slot(&b, &names, 1), Some("alpha"));
    }

    #[test]
    fn atomic_write_leaves_no_tmp() {
        let tmp = tempfile::tempdir().unwrap();
        let b = SlotBindings::default();
        b.save(tmp.path()).unwrap();
        let tmp_file = SlotBindings::path_in(tmp.path()).with_extension("toml.tmp");
        assert!(!tmp_file.exists());
    }
}
