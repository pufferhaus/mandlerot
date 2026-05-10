use crate::scene::{ParamMap, SceneLibrary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Scene,
    Param,
    Preset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    A,
    B,
}

impl Layer {
    pub fn other(self) -> Self {
        match self {
            Layer::A => Layer::B,
            Layer::B => Layer::A,
        }
    }
}

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
    pub active_mode: Mode,
    pub active_layer: Layer,
    pub selected_param_a: u8,
    pub selected_param_b: u8,
    pub audio_bypass: bool,
    pub freeze_active: bool,
    pub tap_tempo_bpm: f32,
    pub active_preset_slot: Option<u8>,
    pub preset_dirty: bool,
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
            active_mode: Mode::Scene,
            active_layer: Layer::A,
            selected_param_a: 0,
            selected_param_b: 0,
            audio_bypass: false,
            freeze_active: false,
            tap_tempo_bpm: 0.0,
            active_preset_slot: None,
            preset_dirty: false,
        })
    }

    pub fn selected_param(&self) -> u8 {
        match self.active_layer {
            Layer::A => self.selected_param_a,
            Layer::B => self.selected_param_b,
        }
    }

    pub fn set_selected_param(&mut self, slot: u8) {
        match self.active_layer {
            Layer::A => self.selected_param_a = slot,
            Layer::B => self.selected_param_b = slot,
        }
    }

    pub fn active_layer_state(&self) -> &LayerState {
        match self.active_layer {
            Layer::A => &self.layer_a,
            Layer::B => &self.layer_b,
        }
    }

    pub fn active_layer_state_mut(&mut self) -> &mut LayerState {
        match self.active_layer {
            Layer::A => &mut self.layer_a,
            Layer::B => &mut self.layer_b,
        }
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

#[cfg(test)]
mod tests_plan2 {
    use super::*;
    use crate::scene::SceneMeta;

    #[test]
    fn shared_state_starts_in_scene_mode_layer_a() {
        let lib = test_lib();
        let s = SharedState::from_initial(&lib, "solid", "solid", 0.0, BlendMode::Mix).unwrap();
        assert_eq!(s.active_mode, Mode::Scene);
        assert_eq!(s.active_layer, Layer::A);
        assert_eq!(s.selected_param_a, 0);
        assert_eq!(s.selected_param_b, 0);
        assert!(!s.audio_bypass);
        assert!(!s.freeze_active);
        assert_eq!(s.tap_tempo_bpm, 0.0);
        assert_eq!(s.active_preset_slot, None);
        assert!(!s.preset_dirty);
    }

    fn test_lib() -> SceneLibrary {
        // Inline minimal library so the test doesn't touch disk
        let meta = SceneMeta::parse(
            "name = \"solid\"\n[[params]]\nslot = 0\nname = \"red\"\nmin = 0.0\nmax = 1.0\ndefault = 1.0\n",
            "inline",
        )
        .unwrap();
        let mut lib = SceneLibrary::default();
        lib.upsert(
            "solid",
            crate::scene::LoadedScene {
                meta,
                fragment_body: "void main() { gl_FragColor = vec4(1.0); }".into(),
                source_path: std::path::PathBuf::from("inline"),
            },
        );
        lib
    }
}
