use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::error::Result;

/// Resolve the directory where user-writable state lives.
///
/// Order:
///   1. `$MANDLEROT_STATE_DIR` (set by the systemd unit on the Pi).
///   2. `<exec_dir>/.config/mandleROT/` next to the binary (dev fallback).
///   3. `./.config/mandleROT/` relative to CWD (last-ditch).
///
/// The returned directory is created if it doesn't already exist.
pub fn user_state_dir() -> PathBuf {
    if let Some(env_dir) = std::env::var_os("MANDLEROT_STATE_DIR") {
        let p = PathBuf::from(env_dir);
        let _ = std::fs::create_dir_all(&p);
        return p;
    }
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join(".config").join("mandleROT");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub render: RenderConfig,
    pub initial: InitialState,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RenderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    #[serde(default = "default_render_scale")]
    pub render_scale: f32,
}

fn default_render_scale() -> f32 {
    1.0
}

#[derive(Debug, Clone, Deserialize)]
pub struct InitialState {
    pub scene_a: String,
    pub scene_b: String,
    #[serde(default = "default_xfade")]
    pub xfade: f32,
    #[serde(default = "default_blend")]
    pub blend_mode: String,
}

fn default_xfade() -> f32 {
    0.0
}
fn default_blend() -> String {
    "mix".to_string()
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let s = std::fs::read_to_string(path)?;
        let cfg: Config = toml::from_str(&s).map_err(|e| crate::Error::SceneMeta {
            file: path.display().to_string(),
            source: e,
        })?;
        Ok(cfg)
    }

    pub fn xfade_or(&self) -> f32 {
        self.initial.xfade
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_config() {
        let s = r#"
            [render]
            width = 720
            height = 480
            fps = 30

            [initial]
            scene_a = "plasma"
            scene_b = "mandelbrot"
        "#;
        let cfg: Config = toml::from_str(s).unwrap();
        assert_eq!(cfg.render.width, 720);
        assert_eq!(cfg.initial.scene_a, "plasma");
        assert_eq!(cfg.initial.xfade, 0.0);
        assert_eq!(cfg.initial.blend_mode, "mix");
    }

    #[test]
    fn xfade_default_is_zero() {
        let s = r#"
            [render]
            width = 720
            height = 480
            fps = 30
            [initial]
            scene_a = "a"
            scene_b = "b"
        "#;
        let cfg: Config = toml::from_str(s).unwrap();
        assert_eq!(cfg.xfade_or(), 0.0);
    }
}
