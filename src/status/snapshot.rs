//! Lean per-frame snapshot of the data the status panel actually renders.
//!
//! The status worker thread used to receive a full `SharedState::clone()`
//! every render frame — which dragged 18 `String` allocations
//! (`Vec<ParamDef>` cloning per layer) plus a `SlotBindings` clone through
//! the global allocator at 30 Hz. `PanelSnapshot` strips that down to a
//! fixed-shape struct: every per-param field that compose reads
//! (name, audio route, value, min, max) lives inline, so the only
//! per-frame heap allocations are the two scene-name strings.
//!
//! Trade-off chosen: param names cap at 8 bytes inline (matching the
//! panel's `truncate(name, 8)` display width). Names longer than that are
//! truncated when the snapshot is built; the render is byte-identical.

use crate::scene::AudioRoute;
use crate::state::{BlendMode, Layer, LayerState, Mode, SharedState};

/// Which chromakey preset (or custom colour) is active. Used by the top-bar
/// `KEY:` chip on the status panel.
#[derive(Debug, Clone, Copy, Default)]
pub enum ChromakeyChip {
    #[default]
    Off,
    Green,
    Magenta,
    Blue,
    Yellow,
    Custom,
}

impl ChromakeyChip {
    pub fn from_state(s: &crate::render::chromakey::ChromakeyState) -> Self {
        if !s.enabled {
            return Self::Off;
        }
        let c = s.key_color;
        let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
        if near(c[0], 0.0) && near(c[1], 1.0) && near(c[2], 0.0) {
            Self::Green
        } else if near(c[0], 1.0) && near(c[1], 0.0) && near(c[2], 1.0) {
            Self::Magenta
        } else if near(c[0], 0.0) && near(c[1], 0.0) && near(c[2], 1.0) {
            Self::Blue
        } else if near(c[0], 1.0) && near(c[1], 1.0) && near(c[2], 0.0) {
            Self::Yellow
        } else {
            Self::Custom
        }
    }

    pub fn as_chip(&self) -> &'static str {
        match self {
            Self::Off => "KEY:--",
            Self::Green => "KEY:G ",
            Self::Magenta => "KEY:M ",
            Self::Blue => "KEY:B ",
            Self::Yellow => "KEY:Y ",
            Self::Custom => "KEY:??",
        }
    }
}

/// Maximum inline width for a param name. Compose already truncates to 8
/// chars for the column it renders into, so we store the truncated bytes
/// directly with no heap.
pub const NAME_INLINE: usize = 8;

/// One slot's worth of param state for the panel. `present == false` means
/// the scene didn't declare this slot — compose renders the `--` placeholder.
#[derive(Debug, Clone, Copy)]
pub struct PanelParam {
    pub present: bool,
    pub route: AudioRoute,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    name_buf: [u8; NAME_INLINE],
    name_len: u8,
}

impl Default for PanelParam {
    fn default() -> Self {
        Self {
            present: false,
            route: AudioRoute::None,
            value: 0.0,
            min: 0.0,
            max: 0.0,
            name_buf: [0u8; NAME_INLINE],
            name_len: 0,
        }
    }
}

impl PanelParam {
    /// Borrowed view of the inline name. Always valid UTF-8 because the
    /// builder copies bytes from a Rust `&str` and truncates at byte
    /// boundaries up to `NAME_INLINE`. ASCII-only param names (the
    /// convention enforced by `scenes/*.toml`) make this trivially safe.
    pub fn name(&self) -> &str {
        let n = self.name_len as usize;
        debug_assert!(n <= NAME_INLINE);
        // SAFETY: bytes were copied from a valid UTF-8 source on a char
        // boundary; ASCII subset is guaranteed by the scene-meta loader.
        std::str::from_utf8(&self.name_buf[..n]).unwrap_or("")
    }
}

/// Per-layer panel data: scene name + fixed-size param array.
#[derive(Debug, Clone)]
pub struct LayerSnapshot {
    pub scene_name: String,
    pub params: [PanelParam; 9],
}

impl LayerSnapshot {
    fn from_layer(layer: &LayerState) -> Self {
        let mut params: [PanelParam; 9] = Default::default();
        for d in layer.params.defs() {
            let slot = d.slot as usize;
            if slot >= 9 {
                continue;
            }
            let value = layer.params.get(&d.name).unwrap_or(d.default);
            let mut name_buf = [0u8; NAME_INLINE];
            let bytes = d.name.as_bytes();
            // Char-aware cap: walk chars until we'd overflow. Most param
            // names are ASCII so this loop runs at most NAME_INLINE
            // iterations.
            let mut len = 0usize;
            for (idx, _ch) in d.name.char_indices() {
                let end = d.name[idx..]
                    .chars()
                    .next()
                    .map(|c| idx + c.len_utf8())
                    .unwrap_or(idx);
                if end > NAME_INLINE {
                    break;
                }
                len = end;
            }
            name_buf[..len].copy_from_slice(&bytes[..len]);
            params[slot] = PanelParam {
                present: true,
                route: d.audio_route,
                value,
                min: d.min,
                max: d.max,
                name_buf,
                name_len: len as u8,
            };
        }
        Self {
            scene_name: layer.scene_name.clone(),
            params,
        }
    }
}

/// Everything compose.rs reads from `SharedState`, in a fixed shape.
/// Constructing one is O(params per layer) — typically ~9 — and allocates
/// exactly two `String`s (the two scene names).
#[derive(Debug, Clone)]
pub struct PanelSnapshot {
    pub mode: Mode,
    pub layer: Layer,
    pub blend_mode: BlendMode,
    pub audio_bypass: bool,
    pub xfade: f32,
    pub bpm: f32,
    pub audio_bands: [f32; 5],
    pub selected_a: u8,
    pub selected_b: u8,
    pub active_look_slot: Option<u8>,
    /// True when the active Look slot owns a post-FX snapshot with active=true.
    /// Defaulted to false by `from_state`; the main loop overrides via
    /// `LookStore::is_bound_active` before sending the snapshot.
    pub look_postfx_bound: bool,
    /// Snapshot of `SharedState::chromakey` flattened to what the top-bar
    /// chip needs to render.
    pub chromakey_chip: ChromakeyChip,
    pub layer_a: LayerSnapshot,
    pub layer_b: LayerSnapshot,
    /// Current video-capture status surfaced to the top-bar `VID:` chip.
    /// Defaulted to `NoDevice` by `from_state`; the main render loop
    /// overrides this with the live `VideoHandle::status()` before sending
    /// the snapshot to the status worker.
    pub video_status: crate::video::VideoStatus,
}

impl PanelSnapshot {
    pub fn from_state(state: &SharedState) -> Self {
        Self {
            mode: state.active_mode,
            layer: state.active_layer,
            blend_mode: state.blend_mode,
            audio_bypass: state.audio_bypass,
            xfade: state.xfade,
            bpm: state.tap_tempo_bpm,
            audio_bands: state.audio_bands,
            selected_a: state.selected_param_a,
            selected_b: state.selected_param_b,
            active_look_slot: state.active_look_slot,
            look_postfx_bound: false,
            chromakey_chip: ChromakeyChip::Off,
            layer_a: LayerSnapshot::from_layer(&state.layer_a),
            layer_b: LayerSnapshot::from_layer(&state.layer_b),
            video_status: crate::video::VideoStatus::NoDevice,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{LoadedScene, SceneLibrary, SceneMeta};

    fn lib() -> SceneLibrary {
        let mut lib = SceneLibrary::default();
        let meta = SceneMeta::parse(
            "name = \"plasma\"\n[[params]]\nslot = 0\nname = \"hue\"\nmin = 0.0\nmax = 1.0\ndefault = 0.5\n",
            "x",
        )
        .unwrap();
        lib.upsert(
            "plasma",
            LoadedScene {
                meta,
                fragment_body: "void main() {}".into(),
                source_path: std::path::PathBuf::from("inline"),
            },
        );
        let meta = SceneMeta::parse(
            "name = \"solid\"\n[[params]]\nslot = 0\nname = \"red\"\nmin = 0.0\nmax = 1.0\ndefault = 1.0\n",
            "x",
        )
        .unwrap();
        lib.upsert(
            "solid",
            LoadedScene {
                meta,
                fragment_body: "void main() {}".into(),
                source_path: std::path::PathBuf::from("inline"),
            },
        );
        lib
    }

    fn state() -> SharedState {
        SharedState::from_initial(&lib(), "plasma", "solid", 0.0, BlendMode::Mix).unwrap()
    }

    #[test]
    fn from_state_carries_scene_names() {
        let s = state();
        let snap = PanelSnapshot::from_state(&s);
        assert_eq!(snap.layer_a.scene_name, "plasma");
        assert_eq!(snap.layer_b.scene_name, "solid");
    }

    #[test]
    fn from_state_fills_present_slot_with_param_data() {
        let s = state();
        let snap = PanelSnapshot::from_state(&s);
        let p = &snap.layer_a.params[0];
        assert!(p.present);
        assert_eq!(p.name(), "hue");
        assert!((p.value - 0.5).abs() < 1e-6);
        assert_eq!(p.min, 0.0);
        assert_eq!(p.max, 1.0);
    }

    #[test]
    fn from_state_marks_unused_slots_absent() {
        let s = state();
        let snap = PanelSnapshot::from_state(&s);
        for slot in 1..9 {
            assert!(!snap.layer_a.params[slot].present);
        }
    }

    #[test]
    fn name_longer_than_inline_truncates_to_eight_bytes() {
        // Synthesise a meta with a long param name to exercise the cap.
        let m = SceneMeta::parse(
            "name = \"x\"\n[[params]]\nslot = 0\nname = \"longnameindeed\"\nmin = 0.0\nmax = 1.0\ndefault = 0.0\n",
            "inline",
        )
        .unwrap();
        let layer = LayerState {
            scene_name: "x".to_string(),
            params: crate::scene::ParamMap::from_scene(&m),
        };
        let snap = LayerSnapshot::from_layer(&layer);
        assert_eq!(snap.params[0].name(), "longname");
    }
}
