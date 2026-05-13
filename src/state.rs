use crate::preset::SlotBindings;
use crate::scene::{ParamMap, SceneLibrary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Scene,
    Param,
    /// Combined A+B scene save/recall slots — the VJ term is "Look".
    Look,
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
    Overlay = 5,
    HardLight = 6,
    Lighten = 7,
    Darken = 8,
    Exclusion = 9,
    Subtract = 10,
    LinearBurn = 11,
    SoftLight = 12,
    ColorDodge = 13,
    ColorBurn = 14,
    Hue = 15,
    Saturation = 16,
    Color = 17,
    Luminosity = 18,
}

impl BlendMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "mix" => Some(Self::Mix),
            "add" => Some(Self::Add),
            "multiply" | "mult" => Some(Self::Multiply),
            "screen" => Some(Self::Screen),
            "difference" | "diff" => Some(Self::Difference),
            "overlay" => Some(Self::Overlay),
            "hardlight" | "hardlt" => Some(Self::HardLight),
            "lighten" | "lgt" => Some(Self::Lighten),
            "darken" | "drk" => Some(Self::Darken),
            "exclusion" | "excl" => Some(Self::Exclusion),
            "subtract" | "sub" => Some(Self::Subtract),
            "linearburn" | "linbn" => Some(Self::LinearBurn),
            "softlight" | "softlt" => Some(Self::SoftLight),
            "colordodge" | "coldodge" | "coldg" => Some(Self::ColorDodge),
            "colorburn" | "colburn" | "colbn" => Some(Self::ColorBurn),
            "hue" => Some(Self::Hue),
            "saturation" | "sat" => Some(Self::Saturation),
            "color" => Some(Self::Color),
            "luminosity" | "lumin" => Some(Self::Luminosity),
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
    /// `[bass, lomid, himid, treble, mid]` — see `audio::bands::BAND_*`.
    /// The first four indices stay the legacy layout so `u_audio.xyzw`
    /// (vec4 uniform) maps to them; `mid` at index 4 is exposed via the
    /// standalone `u_audio_mid` scalar uniform.
    pub audio_bands: [f32; 5],
    pub trigger: f32,
    pub active_mode: Mode,
    pub active_layer: Layer,
    pub selected_param_a: u8,
    pub selected_param_b: u8,
    pub audio_bypass: bool,
    pub freeze_active: bool,
    pub tap_tempo_bpm: f32,
    pub active_look_slot: Option<u8>,
    pub look_dirty: bool,
    pub last_action_label: String,
    pub status_overlay_visible: bool,
    /// User-bound slot → scene mappings. Editable via the in-app menu.
    pub slot_bindings: SlotBindings,
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
            audio_bands: [0.0; 5],
            trigger: 0.0,
            active_mode: Mode::Scene,
            active_layer: Layer::A,
            selected_param_a: 0,
            selected_param_b: 0,
            audio_bypass: false,
            freeze_active: false,
            tap_tempo_bpm: 0.0,
            active_look_slot: None,
            look_dirty: false,
            last_action_label: String::new(),
            status_overlay_visible: false,
            slot_bindings: SlotBindings::default(),
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
        assert_eq!(BlendMode::Overlay.as_int(), 5);
        assert_eq!(BlendMode::LinearBurn.as_int(), 11);
    }

    #[test]
    fn blend_mode_parses_tier1_names_and_aliases() {
        // Canonical names round-trip.
        for (s, expect) in [
            ("overlay", BlendMode::Overlay),
            ("hardlight", BlendMode::HardLight),
            ("lighten", BlendMode::Lighten),
            ("darken", BlendMode::Darken),
            ("exclusion", BlendMode::Exclusion),
            ("subtract", BlendMode::Subtract),
            ("linearburn", BlendMode::LinearBurn),
        ] {
            assert_eq!(BlendMode::parse(s), Some(expect), "canonical {s}");
        }
        // Short aliases match the 6-char status panel tags so users can type
        // either name when hand-editing a Look.
        for (s, expect) in [
            ("hardlt", BlendMode::HardLight),
            ("lgt", BlendMode::Lighten),
            ("drk", BlendMode::Darken),
            ("excl", BlendMode::Exclusion),
            ("sub", BlendMode::Subtract),
            ("linbn", BlendMode::LinearBurn),
        ] {
            assert_eq!(BlendMode::parse(s), Some(expect), "alias {s}");
        }
    }

    #[test]
    fn blend_mode_parses_tier2_names_and_aliases() {
        for (s, expect) in [
            ("softlight", BlendMode::SoftLight),
            ("colordodge", BlendMode::ColorDodge),
            ("colorburn", BlendMode::ColorBurn),
            ("hue", BlendMode::Hue),
            ("saturation", BlendMode::Saturation),
            ("color", BlendMode::Color),
            ("luminosity", BlendMode::Luminosity),
        ] {
            assert_eq!(BlendMode::parse(s), Some(expect), "canonical {s}");
        }
        for (s, expect) in [
            ("softlt", BlendMode::SoftLight),
            ("coldg", BlendMode::ColorDodge),
            ("coldodge", BlendMode::ColorDodge),
            ("colbn", BlendMode::ColorBurn),
            ("colburn", BlendMode::ColorBurn),
            ("sat", BlendMode::Saturation),
            ("lumin", BlendMode::Luminosity),
        ] {
            assert_eq!(BlendMode::parse(s), Some(expect), "alias {s}");
        }
    }
}

#[cfg(test)]
mod tests_plan3 {
    use super::*;

    fn test_lib() -> crate::scene::SceneLibrary {
        use crate::scene::SceneMeta;
        let meta = SceneMeta::parse(
            "name = \"solid\"\n[[params]]\nslot = 0\nname = \"red\"\nmin = 0.0\nmax = 1.0\ndefault = 1.0\n",
            "inline",
        )
        .unwrap();
        let mut lib = crate::scene::SceneLibrary::default();
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

    #[test]
    fn shared_state_has_status_fields() {
        let lib = test_lib();
        let s = SharedState::from_initial(&lib, "solid", "solid", 0.0, BlendMode::Mix).unwrap();
        assert_eq!(s.last_action_label, "");
        assert!(!s.status_overlay_visible);
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
        assert_eq!(s.active_look_slot, None);
        assert!(!s.look_dirty);
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
