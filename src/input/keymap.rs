//! Keymap parsing and `(RawKey, &ModalState) → Action` lookup.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::action::{Action, MenuKind};
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
    /// "000" key on the cheap USB numpad we ship the project against. Acts
    /// as a held shift-equivalent so a single key on the pad can second-
    /// meaning any other key. (NumLock was previously a parallel modifier
    /// but is now a primary key — SceneCycleActive previous — because the
    /// rotated pad layout needs every primary key.)
    Numpad000,
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
                Some("Numpad000") => Modifier::Numpad000,
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
    if let Some(rest) = label.strip_prefix("OpenMenu:") {
        let kind = match rest {
            "Settings" => MenuKind::Settings,
            other => return Err(Error::Backend(format!("unknown menu: {other}"))),
        };
        return Ok(ActionTemplate::Static(Action::OpenMenu(kind)));
    }
    if let Some(rest) = label.strip_prefix("SceneCycle:") {
        // SceneCycle:A:1, SceneCycle:B:-1, or SceneCycle:active:N.
        let parts: Vec<&str> = rest.split(':').collect();
        if parts.len() != 2 {
            return Err(Error::Backend(format!("bad SceneCycle label: {label}")));
        }
        let dir: i8 = parts[1]
            .parse()
            .map_err(|_| Error::Backend(format!("bad dir: {}", parts[1])))?;
        if parts[0] == "active" {
            return Ok(ActionTemplate::Static(Action::SceneCycleActive { dir }));
        }
        if parts[0] == "other" {
            return Ok(ActionTemplate::Static(Action::SceneCycleOther { dir }));
        }
        let layer = match parts[0] {
            "A" => Layer::A,
            "B" => Layer::B,
            _ => return Err(Error::Backend(format!("bad layer: {}", parts[0]))),
        };
        return Ok(ActionTemplate::Static(Action::SceneCycle { layer, dir }));
    }
    let action = match label {
        "AdvanceMode" => Action::AdvanceMode,
        "ToggleLayer" => Action::ToggleLayer,
        "XfadeMinus" => Action::XfadeMinus,
        "XfadePlus" => Action::XfadePlus,
        "ParamMinus" => Action::ParamMinus,
        "ParamPlus" => Action::ParamPlus,
        "ParamAudioMinus" => Action::ParamAudioCycle { dir: -1 },
        "ParamAudioPlus" => Action::ParamAudioCycle { dir: 1 },
        "Trigger" => Action::Trigger,
        "BlendCycle" => Action::BlendCycle,
        "FreezeToggle" => Action::FreezeToggle,
        "TapTempo" => Action::TapTempo,
        "AudioBypass" => Action::AudioBypass,
        "ChromakeyToggle" => Action::ChromakeyToggle,
        "Panic" => Action::Panic,
        "ReloadAllScenes" => Action::ReloadAllScenes,
        "ResetAllParams" => Action::ResetAllParams,
        "DebugOverlayToggle" => Action::DebugOverlayToggle,
        other => return Err(Error::Backend(format!("unknown Action label: {other}"))),
    };
    Ok(ActionTemplate::Static(action))
}

fn materialize(template: &ActionTemplate, held: Modifier) -> Action {
    match template {
        ActionTemplate::Static(a) => a.clone(),
        ActionTemplate::Slot(n) => {
            let other_layer = matches!(held, Modifier::Shift | Modifier::Numpad000);
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
            audio_bands: [0.0; 5],
            trigger: 0.0,
            active_mode: crate::state::Mode::Scene,
            active_layer: crate::state::Layer::A,
            selected_param_a: 0,
            selected_param_b: 0,
            audio_bypass: false,
            freeze_active: false,
            tap_tempo_bpm: 0.0,
            active_look_slot: None,
            look_dirty: false,
            last_action_label: String::new(),
            status_overlay_visible: false,
            slot_bindings: crate::preset::SlotBindings::default(),
            chromakey: crate::render::chromakey::ChromakeyState::default(),
        }
    }

    #[test]
    fn tab_maps_to_advance_mode() {
        let km = parsed();
        let s = dummy_state();
        assert_eq!(
            km.lookup("Tab", Modifier::None, &s),
            Some(Action::AdvanceMode)
        );
    }

    #[test]
    fn slot_with_no_modifier_is_active_layer() {
        let km = parsed();
        let s = dummy_state();
        let a = km.lookup("1", Modifier::None, &s).unwrap();
        assert_eq!(
            a,
            Action::Slot {
                n: 1,
                other_layer: false
            }
        );
    }

    #[test]
    fn slot_with_shift_is_other_layer() {
        let km = parsed();
        let s = dummy_state();
        let a = km.lookup("1", Modifier::Shift, &s).unwrap();
        assert_eq!(
            a,
            Action::Slot {
                n: 1,
                other_layer: true
            }
        );
    }

    #[test]
    fn numpad_digits_are_remapped_for_rotated_pad() {
        // 3x3 block reads naturally when the pad is on its left side.
        // Physical key → assigned slot: 9→1, 6→2, 3→3, 8→4, 5→5, 2→6,
        // 7→7, 4→8, 1→9.
        let km = parsed();
        let s = dummy_state();
        for (physical, expected_slot) in [
            ("Numpad9", 1),
            ("Numpad6", 2),
            ("Numpad3", 3),
            ("Numpad8", 4),
            ("Numpad5", 5),
            ("Numpad2", 6),
            ("Numpad7", 7),
            ("Numpad4", 8),
            ("Numpad1", 9),
        ] {
            let a = km.lookup(physical, Modifier::None, &s).unwrap();
            assert_eq!(
                a,
                Action::Slot {
                    n: expected_slot,
                    other_layer: false,
                },
                "{physical} should map to slot {expected_slot}"
            );
        }
    }

    #[test]
    fn previous_scene_lives_on_numpad_multiply() {
        // The dead NumLock corner key got retired in favour of `*`
        // (NumpadMultiply) for the previous-scene cycle action.
        let km = parsed();
        let s = dummy_state();
        assert_eq!(
            km.lookup("NumpadMultiply", Modifier::None, &s),
            Some(Action::SceneCycleActive { dir: -1 })
        );
    }

    #[test]
    fn backspace_is_trigger() {
        // Bksp moved to Trigger duty; the scene-cycle-next action it used
        // to hold has no numpad-side replacement at the moment.
        let km = parsed();
        let s = dummy_state();
        assert_eq!(km.lookup("Backspace", Modifier::None, &s), Some(Action::Trigger));
    }

    #[test]
    fn numpad000_modifier_overlays_blend_and_other_prev() {
        let km = parsed();
        let s = dummy_state();
        // 000+* mirrors PREV-active to PREV-other.
        assert_eq!(
            km.lookup("NumpadMultiply", Modifier::Numpad000, &s),
            Some(Action::SceneCycleOther { dir: -1 })
        );
        // 000+Bksp is the only numpad path to BlendCycle now that the
        // unmodified `*` is repurposed.
        assert_eq!(
            km.lookup("Backspace", Modifier::Numpad000, &s),
            Some(Action::BlendCycle)
        );
    }

    #[test]
    fn numpad000_plus_five_resets_all_params() {
        // `000+5` overrides the implicit "Slot 5 on other layer" template
        // and becomes a one-shot reset for everything on the active layer.
        let km = parsed();
        let s = dummy_state();
        assert_eq!(
            km.lookup("Numpad5", Modifier::Numpad000, &s),
            Some(Action::ResetAllParams)
        );
    }

    #[test]
    fn unknown_key_returns_none() {
        let km = parsed();
        let s = dummy_state();
        assert!(km.lookup("Quux", Modifier::None, &s).is_none());
    }

    #[test]
    fn esc_panics_single_press() {
        let km = parsed();
        let s = dummy_state();
        // Esc keeps its keyboard Panic role. Backspace is repurposed as
        // SceneCycleActive next — see `backspace_is_scene_cycle_active_next`.
        // A double-tap of either still fires Panic via the global watcher
        // in main.rs (not exercised here).
        assert_eq!(km.lookup("Esc", Modifier::None, &s), Some(Action::Panic));
    }

    #[test]
    fn numpad_decimal_unmodified_is_advance_mode() {
        // `.` is the primary mode-cycle key in the new layout — bottom row,
        // single press. AudioBypass moved off this slot onto a 000-held combo.
        let km = parsed();
        let s = dummy_state();
        assert_eq!(
            km.lookup("NumpadDecimal", Modifier::None, &s),
            Some(Action::AdvanceMode)
        );
    }

    #[test]
    fn numpad000_modifier_unlocks_orphan_actions() {
        // Holding the `000` key on the cheap pad turns three bottom-row
        // keys into less-frequent actions.
        let km = parsed();
        let s = dummy_state();
        assert_eq!(
            km.lookup("NumpadEnter", Modifier::Numpad000, &s),
            Some(Action::AudioBypass)
        );
        assert_eq!(
            km.lookup("Numpad0", Modifier::Numpad000, &s),
            Some(Action::FreezeToggle)
        );
        assert_eq!(
            km.lookup("NumpadDecimal", Modifier::Numpad000, &s),
            Some(Action::OpenMenu(MenuKind::Settings))
        );
        // The digit row flips to "other layer" under 000 — except Numpad5,
        // which is overridden to ResetAllParams (covered by its own test).
        assert_eq!(
            km.lookup("Numpad1", Modifier::Numpad000, &s),
            Some(Action::Slot {
                n: 9,
                other_layer: true
            })
        );
    }


    #[test]
    fn enter_toggles_layer_backslash_advances_mode() {
        let km = parsed();
        let s = dummy_state();
        assert_eq!(
            km.lookup("NumpadEnter", Modifier::None, &s),
            Some(Action::ToggleLayer)
        );
        // Backslash got repurposed from ToggleLayer to AdvanceMode; Enter
        // alone still covers layer-toggle.
        assert_eq!(
            km.lookup("Backslash", Modifier::None, &s),
            Some(Action::AdvanceMode)
        );
    }
}
