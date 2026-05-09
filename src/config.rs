use serde::Deserialize;
use std::path::Path;

use crate::error::Result;

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

impl Config {
    pub fn xfade_or(&self) -> f32 {
        self.initial.xfade
    }
}
