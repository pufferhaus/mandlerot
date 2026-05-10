//! Keymap parsing and `(RawKey, &ModalState) → Action` lookup.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::action::Action;
use crate::error::{Error, Result};
use crate::state::{Layer, SharedState};

/// Identifier for a physical key, platform-independent.
///
/// Use a `String` here rather than an enum because the universe of named keys
/// across winit and evdev is large and rarely warrants exhaustive matching.
/// The string format is documented in `keymap.toml`.
pub type RawKey = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    None,
    Shift,
    NumLock,
}

#[derive(Debug, Deserialize)]
struct KeymapFile {
    binding: Vec<BindingEntry>,
}

#[derive(Debug, Deserialize)]
struct BindingEntry {
    key: String,
    action: String,
    #[serde(default)]
    modifier: Option<String>,
}

#[derive(Debug, Default)]
pub struct KeyMap {
    /// (key, modifier) → ActionTemplate
    entries: HashMap<(RawKey, Modifier), ActionTemplate>,
}

/// An `Action` shape stored in the keymap. Some Actions need runtime context
/// (e.g. `Slot.other_layer` depends on whether the modifier was held), so we
/// pre-parse the keymap line into a template and resolve at lookup time.
#[derive(Debug, Clone)]
enum ActionTemplate {
    Static(Action),
    /// Slot:N — resolve other_layer from runtime modifier state.
    Slot(u8),
}

impl KeyMap {
    pub fn load(path: &Path) -> Result<Self> {
        let s = std::fs::read_to_string(path)?;
        Self::parse(&s)
    }

    pub fn parse(s: &str) -> Result<Self> {
        let file: KeymapFile = toml::from_str(s).map_err(|e| Error::SceneMeta {
            file: "keymap.toml".into(),
            source: e,
        })?;
        let mut entries = HashMap::new();
        for b in file.binding {
            let modifier = match b.modifier.as_deref() {
                None => Modifier::None,
                Some("Shift") => Modifier::Shift,
                Some("NumLock") => Modifier::NumLock,
                Some(other) => {
                    return Err(Error::Backend(format!("unknown modifier: {other}")));
                }
            };
            let template = parse_action_label(&b.action)?;
            entries.insert((b.key, modifier), template);
        }
        Ok(Self { entries })
    }

    /// Look up an action for a key event. `held_modifier` is the modifier
    /// currently held when the key was pressed.
    pub fn lookup(
        &self,
        key: &str,
        held_modifier: Modifier,
        _state: &SharedState,
    ) -> Option<Action> {
        // Try exact-modifier match first, then fall back to None modifier.
        let primary = self.entries.get(&(key.to_string(), held_modifier));
        let fallback = self.entries.get(&(key.to_string(), Modifier::None));
        let template = primary.or(fallback)?;
        Some(materialize(template, held_modifier))
    }
}

fn parse_action_label(label: &str) -> Result<ActionTemplate> {
    if let Some(rest) = label.strip_prefix("Slot:") {
        let n: u8 = rest
            .parse()
            .map_err(|_| Error::Backend(format!("bad Slot label: {label}")))?;
        if !(1..=9).contains(&n) {
            return Err(Error::Backend(format!("Slot N out of range: {n}")));
        }
        return Ok(ActionTemplate::Slot(n));
    }
    if let Some(rest) = label.strip_prefix("SceneCycle:") {
        // SceneCycle:A:1 or SceneCycle:B:-1
        let parts: Vec<&str> = rest.split(':').collect();
        if parts.len() != 2 {
            return Err(Error::Backend(format!("bad SceneCycle label: {label}")));
        }
        let layer = match parts[0] {
            "A" => Layer::A,
            "B" => Layer::B,
            _ => return Err(Error::Backend(format!("bad layer: {}", parts[0]))),
        };
        let dir: i8 = parts[1]
            .parse()
            .map_err(|_| Error::Backend(format!("bad dir: {}", parts[1])))?;
        return Ok(ActionTemplate::Static(Action::SceneCycle { layer, dir }));
    }
    let action = match label {
        "AdvanceMode" => Action::AdvanceMode,
        "ToggleLayer" => Action::ToggleLayer,
        "XfadeMinus" => Action::XfadeMinus,
        "XfadePlus" => Action::XfadePlus,
        "ParamMinus" => Action::ParamMinus,
        "ParamPlus" => Action::ParamPlus,
        "Trigger" => Action::Trigger,
        "BlendCycle" => Action::BlendCycle,
        "FreezeToggle" => Action::FreezeToggle,
        "TapTempo" => Action::TapTempo,
        "AudioBypass" => Action::AudioBypass,
        "Panic" => Action::Panic,
        "ReloadAllScenes" => Action::ReloadAllScenes,
        "ResetAllParams" => Action::ResetAllParams,
        "DebugOverlayToggle" => {
            // Not a state-mutating action; the desktop adapter handles this
            // synchronously. Map to a trigger that the apply layer ignores.
            return Ok(ActionTemplate::Static(Action::Trigger));
        }
        other => return Err(Error::Backend(format!("unknown Action label: {other}"))),
    };
    Ok(ActionTemplate::Static(action))
}

fn materialize(template: &ActionTemplate, held: Modifier) -> Action {
    match template {
        ActionTemplate::Static(a) => a.clone(),
        ActionTemplate::Slot(n) => {
            let other_layer = matches!(held, Modifier::Shift | Modifier::NumLock);
            Action::Slot { n: *n, other_layer }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::SceneLibrary;
    use crate::state::{BlendMode, SharedState};

    fn parsed() -> KeyMap {
        KeyMap::parse(include_str!("../../keymap.toml")).unwrap()
    }

    fn dummy_state() -> SharedState {
        let _lib = SceneLibrary::default();
        // SharedState::from_initial requires the lib to have the scenes; build
        // a hand-constructed SharedState instead.
        SharedState {
            layer_a: crate::state::LayerState {
                scene_name: "x".into(),
                params: crate::scene::ParamMap::default(),
            },
            layer_b: crate::state::LayerState {
                scene_name: "x".into(),
                params: crate::scene::ParamMap::default(),
            },
            xfade: 0.0,
            blend_mode: BlendMode::Mix,
            time_secs: 0.0,
            audio_bands: [0.0; 4],
            trigger: 0.0,
            active_mode: crate::state::Mode::Scene,
            active_layer: crate::state::Layer::A,
            selected_param_a: 0,
            selected_param_b: 0,
            audio_bypass: false,
            freeze_active: false,
            tap_tempo_bpm: 0.0,
            active_preset_slot: None,
            preset_dirty: false,
            last_action_label: String::new(),
        }
    }

    #[test]
    fn tab_maps_to_advance_mode() {
        let km = parsed();
        let s = dummy_state();
        assert_eq!(km.lookup("Tab", Modifier::None, &s), Some(Action::AdvanceMode));
    }

    #[test]
    fn slot_with_no_modifier_is_active_layer() {
        let km = parsed();
        let s = dummy_state();
        let a = km.lookup("1", Modifier::None, &s).unwrap();
        assert_eq!(a, Action::Slot { n: 1, other_layer: false });
    }

    #[test]
    fn slot_with_shift_is_other_layer() {
        let km = parsed();
        let s = dummy_state();
        let a = km.lookup("1", Modifier::Shift, &s).unwrap();
        assert_eq!(a, Action::Slot { n: 1, other_layer: true });
    }

    #[test]
    fn slot_with_numlock_is_other_layer() {
        let km = parsed();
        let s = dummy_state();
        let a = km.lookup("Numpad1", Modifier::NumLock, &s).unwrap();
        assert_eq!(a, Action::Slot { n: 1, other_layer: true });
    }

    #[test]
    fn unknown_key_returns_none() {
        let km = parsed();
        let s = dummy_state();
        assert!(km.lookup("Quux", Modifier::None, &s).is_none());
    }

    #[test]
    fn esc_and_bksp_both_panic() {
        let km = parsed();
        let s = dummy_state();
        assert_eq!(km.lookup("Esc", Modifier::None, &s), Some(Action::Panic));
        assert_eq!(km.lookup("Backspace", Modifier::None, &s), Some(Action::Panic));
    }

    #[test]
    fn enter_and_backslash_both_toggle_layer() {
        let km = parsed();
        let s = dummy_state();
        assert_eq!(km.lookup("NumpadEnter", Modifier::None, &s), Some(Action::ToggleLayer));
        assert_eq!(km.lookup("Backslash", Modifier::None, &s), Some(Action::ToggleLayer));
    }
}
