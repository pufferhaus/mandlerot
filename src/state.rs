use crate::scene::{ParamMap, SceneLibrary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Mix = 0,
    Add = 1,
    Multiply = 2,
    Screen = 3,
    Difference = 4,
}

impl BlendMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "mix" => Some(Self::Mix),
            "add" => Some(Self::Add),
            "multiply" | "mult" => Some(Self::Multiply),
            "screen" => Some(Self::Screen),
            "difference" | "diff" => Some(Self::Difference),
            _ => None,
        }
    }
    pub fn as_int(self) -> i32 {
        self as i32
    }
}

#[derive(Debug, Clone)]
pub struct LayerState {
    pub scene_name: String,
    pub params: ParamMap,
}

#[derive(Debug, Clone)]
pub struct SharedState {
    pub layer_a: LayerState,
    pub layer_b: LayerState,
    pub xfade: f32,
    pub blend_mode: BlendMode,
    pub time_secs: f32,
    pub audio_bands: [f32; 4],
    pub trigger: f32,
}

impl SharedState {
    pub fn from_initial(
        lib: &SceneLibrary,
        scene_a: &str,
        scene_b: &str,
        xfade: f32,
        blend_mode: BlendMode,
    ) -> crate::Result<Self> {
        let a = lib.require(scene_a)?;
        let b = lib.require(scene_b)?;
        Ok(Self {
            layer_a: LayerState {
                scene_name: scene_a.to_string(),
                params: ParamMap::from_scene(&a.meta),
            },
            layer_b: LayerState {
                scene_name: scene_b.to_string(),
                params: ParamMap::from_scene(&b.meta),
            },
            xfade: xfade.clamp(0.0, 1.0),
            blend_mode,
            time_secs: 0.0,
            audio_bands: [0.0; 4],
            trigger: 0.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_mode_parses_known_strings() {
        assert_eq!(BlendMode::parse("mix"), Some(BlendMode::Mix));
        assert_eq!(BlendMode::parse("multiply"), Some(BlendMode::Multiply));
        assert_eq!(BlendMode::parse("mult"), Some(BlendMode::Multiply));
        assert_eq!(BlendMode::parse("nonsense"), None);
    }

    #[test]
    fn blend_mode_int_matches_shader() {
        assert_eq!(BlendMode::Mix.as_int(), 0);
        assert_eq!(BlendMode::Difference.as_int(), 4);
    }
}
