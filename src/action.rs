//! User-issued intents derived from input events.

use crate::state::{BlendMode, Layer};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Number-row slot pressed. Meaning depends on current `Mode`.
    Slot { n: u8, other_layer: bool },
    /// Tab — advance mode (Scene → Param → Preset → Scene).
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
    /// PANIC — both layers to safe-scene, xfade=0.5, audio bypass on.
    Panic,
    /// Force-reload all scenes (dev key F5).
    ReloadAllScenes,
    /// Scene cycle next on a layer (dev keys F2/F3).
    SceneCycle { layer: Layer, dir: i8 },
    /// Set explicit blend mode (used by preset recall).
    SetBlendMode(BlendMode),
    /// Set explicit xfade (used by preset recall).
    SetXfade(f32),
    /// Set explicit scene on a layer (used by Slot in SCENE mode).
    SetSceneByIndex { layer: Layer, index: u8 },
    /// Recall preset slot 1-8 (PRESET mode + slot key, no modifier).
    RecallPreset { slot: u8 },
    /// Save current state to preset slot 1-8 (PRESET mode + slot key + modifier).
    SavePreset { slot: u8 },
    /// Toggle the debug overlay. Currently a no-op until Plan 3 lands the
    /// overlay state; included now so F1 doesn't silently fire `Trigger`.
    DebugOverlayToggle,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_with_modifier_is_distinct() {
        let a = Action::Slot { n: 1, other_layer: false };
        let b = Action::Slot { n: 1, other_layer: true };
        assert_ne!(a, b);
    }

    #[test]
    fn cloneable() {
        let a = Action::AdvanceMode;
        let b = a.clone();
        assert_eq!(a, b);
    }
}
