//! Live-tunable audio parameters shared between the UI thread (writer)
//! and the audio worker thread (reader). All fields stored as `AtomicU32`
//! bit-casts of `f32` for lock-free updates.
//!
//! Persisted to `<state_dir>/audio.toml`. Missing file → defaults.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const FILE_NAME: &str = "audio.toml";

#[derive(Debug)]
pub struct AudioParams {
    noise_floor: AtomicU32,
    gain_bass: AtomicU32,
    gain_lomid: AtomicU32,
    gain_himid: AtomicU32,
    gain_treble: AtomicU32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AudioParamsFile {
    #[serde(default = "default_noise_floor")]
    noise_floor: f32,
    #[serde(default = "default_gain")]
    gain_bass: f32,
    #[serde(default = "default_gain")]
    gain_lomid: f32,
    #[serde(default = "default_gain")]
    gain_himid: f32,
    #[serde(default = "default_gain")]
    gain_treble: f32,
}

pub const DEFAULT_NOISE_FLOOR: f32 = 8.0;
pub const DEFAULT_GAIN: f32 = 1.0;

fn default_noise_floor() -> f32 {
    DEFAULT_NOISE_FLOOR
}
fn default_gain() -> f32 {
    DEFAULT_GAIN
}

impl Default for AudioParamsFile {
    fn default() -> Self {
        Self {
            noise_floor: DEFAULT_NOISE_FLOOR,
            gain_bass: DEFAULT_GAIN,
            gain_lomid: DEFAULT_GAIN,
            gain_himid: DEFAULT_GAIN,
            gain_treble: DEFAULT_GAIN,
        }
    }
}

impl AudioParams {
    pub fn new() -> Arc<Self> {
        Self::from_file(AudioParamsFile::default())
    }

    fn from_file(f: AudioParamsFile) -> Arc<Self> {
        Arc::new(Self {
            noise_floor: AtomicU32::new(f.noise_floor.to_bits()),
            gain_bass: AtomicU32::new(f.gain_bass.to_bits()),
            gain_lomid: AtomicU32::new(f.gain_lomid.to_bits()),
            gain_himid: AtomicU32::new(f.gain_himid.to_bits()),
            gain_treble: AtomicU32::new(f.gain_treble.to_bits()),
        })
    }

    pub fn path_in(state_dir: &Path) -> PathBuf {
        state_dir.join(FILE_NAME)
    }

    pub fn load_or_default(state_dir: &Path) -> Arc<Self> {
        let path = Self::path_in(state_dir);
        let Ok(s) = std::fs::read_to_string(&path) else {
            return Self::new();
        };
        match toml::from_str::<AudioParamsFile>(&s) {
            Ok(f) => Self::from_file(f),
            Err(e) => {
                tracing::warn!("audio.toml parse: {e}; using defaults");
                Self::new()
            }
        }
    }

    pub fn save(&self, state_dir: &Path) -> Result<()> {
        let path = Self::path_in(state_dir);
        std::fs::create_dir_all(state_dir)?;
        let f = AudioParamsFile {
            noise_floor: self.noise_floor(),
            gain_bass: self.gain(0),
            gain_lomid: self.gain(1),
            gain_himid: self.gain(2),
            gain_treble: self.gain(3),
        };
        let body =
            toml::to_string_pretty(&f).map_err(|e| Error::Backend(format!("serialize: {e}")))?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, body)?;
        let fh = std::fs::File::open(&tmp)?;
        fh.sync_all()?;
        drop(fh);
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn noise_floor(&self) -> f32 {
        f32::from_bits(self.noise_floor.load(Ordering::Relaxed))
    }

    pub fn set_noise_floor(&self, v: f32) {
        self.noise_floor.store(v.to_bits(), Ordering::Relaxed);
    }

    /// `band` indexes 0=bass, 1=lomid, 2=himid, 3=treble.
    pub fn gain(&self, band: usize) -> f32 {
        let a = match band {
            0 => &self.gain_bass,
            1 => &self.gain_lomid,
            2 => &self.gain_himid,
            _ => &self.gain_treble,
        };
        f32::from_bits(a.load(Ordering::Relaxed))
    }

    pub fn set_gain(&self, band: usize, v: f32) {
        let a = match band {
            0 => &self.gain_bass,
            1 => &self.gain_lomid,
            2 => &self.gain_himid,
            _ => &self.gain_treble,
        };
        a.store(v.to_bits(), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values_are_sane() {
        let p = AudioParams::new();
        assert_eq!(p.noise_floor(), DEFAULT_NOISE_FLOOR);
        for b in 0..4 {
            assert_eq!(p.gain(b), DEFAULT_GAIN);
        }
    }

    #[test]
    fn set_get_roundtrip() {
        let p = AudioParams::new();
        p.set_noise_floor(3.5);
        p.set_gain(2, 2.25);
        assert_eq!(p.noise_floor(), 3.5);
        assert_eq!(p.gain(2), 2.25);
    }

    #[test]
    fn save_then_load_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let p = AudioParams::new();
        p.set_noise_floor(12.0);
        p.set_gain(0, 1.5);
        p.set_gain(3, 0.5);
        p.save(tmp.path()).unwrap();
        let q = AudioParams::load_or_default(tmp.path());
        assert_eq!(q.noise_floor(), 12.0);
        assert_eq!(q.gain(0), 1.5);
        assert_eq!(q.gain(3), 0.5);
    }

    #[test]
    fn missing_file_returns_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let q = AudioParams::load_or_default(tmp.path());
        assert_eq!(q.noise_floor(), DEFAULT_NOISE_FLOOR);
    }
}
