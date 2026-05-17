//! User-issued intents derived from input events.

use crate::state::{BlendMode, Layer};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Number-row slot pressed. Meaning depends on current `Mode`.
    Slot { n: u8, other_layer: bool },
    /// Tab — advance mode (Scene → Param → Look → Scene).
    AdvanceMode,
    /// Toggle active layer A↔B (Enter on numpad, `\` on keyboard).
    ToggleLayer,
    /// Crossfade nudge negative (toward A).
    XfadeMinus,
    /// Crossfade nudge positive (toward B).
    XfadePlus,
    /// Param decrement on selected slot of active layer.
    ParamMinus,
    /// Param increment on selected slot of active layer.
    ParamPlus,
    /// Cycle audio route on the selected param of the active layer.
    /// `dir = +1` → forward (None→Bass→Lomid→Himid→Treble→Beat→None).
    /// Only meaningful in PARAM mode.
    ParamAudioCycle { dir: i8 },
    /// Reset all params on active layer to scene defaults.
    ResetAllParams,
    /// Cycle blend mode forward.
    BlendCycle,
    /// One-frame trigger pulse.
    Trigger,
    /// Toggle freeze (pauses u_time).
    FreezeToggle,
    /// Tap-tempo: register a tap for BPM derivation.
    TapTempo,
    /// Toggle audio reactivity bypass.
    AudioBypass,
    /// Toggle the chromakey output mode. Persists `chromakey.toml`.
    ChromakeyToggle,
    /// PANIC — both layers to safe-scene, xfade=0.5, audio bypass on.
    Panic,
    /// Force-reload all scenes (dev key F5).
    ReloadAllScenes,
    /// Scene cycle next on a layer (dev keys F2/F3).
    SceneCycle { layer: Layer, dir: i8 },
    /// Scene cycle on whichever layer is currently active. Used by the
    /// numpad's `Backspace` (dir = +1) and `NumLock` (dir = -1) keys so the
    /// operator can walk the library without first having to flip layers.
    SceneCycleActive { dir: i8 },
    /// Scene cycle on the *other* (inactive) layer. Saves the
    /// Enter-cycle-Enter dance when the operator wants to set up the next
    /// layer while still playing the current one. Bound to `000`-modified
    /// versions of the same two keys.
    SceneCycleOther { dir: i8 },
    /// Set explicit blend mode (used by look recall).
    SetBlendMode(BlendMode),
    /// Set explicit xfade (used by look recall).
    SetXfade(f32),
    /// Set explicit scene on a layer (used by Slot in SCENE mode).
    SetSceneByIndex { layer: Layer, index: u8 },
    /// Set scene on a layer by name. Used when slot resolution went through
    /// SlotBindings; `SetSceneByIndex` is reserved for ordinal walks
    /// (`SceneCycle` and the legacy alphabetical fallback).
    SetSceneByName { layer: Layer, name: String },
    /// Recall look slot 1-8 (LOOK mode + slot key, no modifier).
    RecallLook { slot: u8 },
    /// Save current state to look slot 1-8 (LOOK mode + slot key + modifier).
    SaveLook { slot: u8 },
    /// Toggle the debug overlay. Currently a no-op until Plan 3 lands the
    /// overlay state; included now so F1 doesn't silently fire `Trigger`.
    DebugOverlayToggle,
    /// Toggle the Post-FX → Look binding on the currently active Look slot.
    /// Dispatched from the postfx screen on the `b` key. No-op if no Look
    /// is active. Reserved for future global keymap entry too.
    PostFxBindToggle,
    /// Open a named menu screen on top of the status panel. Input is routed
    /// to that screen until it (and any pushed children) are dismissed.
    OpenMenu(MenuKind),
}

/// Identifiers for the top-level menu entry points reachable by a key. Each
/// variant maps to a constructor in `crate::ui::screens`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    Settings,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_with_modifier_is_distinct() {
        let a = Action::Slot {
            n: 1,
            other_layer: false,
        };
        let b = Action::Slot {
            n: 1,
            other_layer: true,
        };
        assert_ne!(a, b);
    }

    #[test]
    fn cloneable() {
        let a = Action::AdvanceMode;
        let b = a.clone();
        assert_eq!(a, b);
    }
}
