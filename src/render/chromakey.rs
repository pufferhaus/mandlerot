//! Chromakey output mode (item 27). The blend shader reads these values
//! through uniforms (`u_key_*`) when `enabled` is true.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const FILE_NAME: &str = "chromakey.toml";

/// Operator-tunable chromakey output state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromakeyState {
    #[serde(default)]
    pub enabled: bool,
    /// Linear RGB triplet in [0, 1].
    #[serde(default = "default_color")]
    pub key_color: [f32; 3],
    /// Luma threshold (Rec. 601 weights). Pixels with luma <= this become the
    /// key color. Spec default 0.04; UI clamps to [0.0, 0.5].
    #[serde(default = "default_luma")]
    pub luma_threshold: f32,
    /// Soft-edge half-width on the luma threshold. 0 == hard step.
    #[serde(default = "default_soft")]
    pub edge_soft: f32,
    /// Subtract the key color's chroma component from non-key pixels so edges
    /// don't get tinted by the mixer's downstream key.
    #[serde(default)]
    pub spill_suppress: bool,
}

fn default_color() -> [f32; 3] { [0.0, 1.0, 0.0] } // chroma green
fn default_luma() -> f32 { 0.04 }
fn default_soft() -> f32 { 0.02 }

impl Default for ChromakeyState {
    fn default() -> Self {
        Self {
            enabled: false,
            key_color: default_color(),
            luma_threshold: default_luma(),
            edge_soft: default_soft(),
            spill_suppress: false,
        }
    }
}

impl ChromakeyState {
    pub fn path_in(state_dir: &Path) -> PathBuf {
        state_dir.join(FILE_NAME)
    }

    /// Read the file if present; otherwise return defaults. Parse errors warn
    /// and fall back to defaults (same policy as `AudioParams::load_or_default`).
    pub fn load_or_default(state_dir: &Path) -> Self {
        let path = Self::path_in(state_dir);
        let Ok(s) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match toml::from_str::<Self>(&s) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("chromakey.toml parse: {e}; using defaults");
                Self::default()
            }
        }
    }

    /// Atomic write: tmp + fsync + rename. Matches `AudioParams::save`.
    pub fn save(&self, state_dir: &Path) -> Result<()> {
        let path = Self::path_in(state_dir);
        std::fs::create_dir_all(state_dir)?;
        let body =
            toml::to_string_pretty(self).map_err(|e| Error::Backend(format!("serialize: {e}")))?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, body)?;
        let fh = std::fs::File::open(&tmp)?;
        fh.sync_all()?;
        drop(fh);
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec() {
        let s = ChromakeyState::default();
        assert!(!s.enabled);
        assert_eq!(s.key_color, [0.0, 1.0, 0.0]);
        assert!((s.luma_threshold - 0.04).abs() < 1e-6);
        assert!((s.edge_soft - 0.02).abs() < 1e-6);
        assert!(!s.spill_suppress);
    }

    #[test]
    fn load_missing_file_returns_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let s = ChromakeyState::load_or_default(tmp.path());
        assert!(!s.enabled);
    }

    #[test]
    fn save_then_load_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = ChromakeyState::default();
        s.enabled = true;
        s.key_color = [1.0, 0.0, 1.0]; // magenta
        s.luma_threshold = 0.06;
        s.edge_soft = 0.03;
        s.spill_suppress = true;
        s.save(tmp.path()).unwrap();
        let reloaded = ChromakeyState::load_or_default(tmp.path());
        assert!(reloaded.enabled);
        assert_eq!(reloaded.key_color, [1.0, 0.0, 1.0]);
        assert!((reloaded.luma_threshold - 0.06).abs() < 1e-6);
        assert!((reloaded.edge_soft - 0.03).abs() < 1e-6);
        assert!(reloaded.spill_suppress);
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(FILE_NAME), b"not toml = =").unwrap();
        let s = ChromakeyState::load_or_default(tmp.path());
        // Falls through to defaults; we only care that it didn't panic.
        assert!(!s.enabled);
    }
}
