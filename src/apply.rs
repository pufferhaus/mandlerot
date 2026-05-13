//! Apply an `Action` to `SharedState`. Pure besides logging.

use crate::action::Action;
use crate::error::Result;
use crate::preset::resolve_slot;
use crate::scene::SceneLibrary;
use crate::state::{BlendMode, Layer, Mode, SharedState};

const BLEND_MODES: [BlendMode; 19] = [
    BlendMode::Mix,
    BlendMode::Add,
    BlendMode::Multiply,
    BlendMode::Screen,
    BlendMode::Difference,
    BlendMode::Overlay,
    BlendMode::HardLight,
    BlendMode::Lighten,
    BlendMode::Darken,
    BlendMode::Exclusion,
    BlendMode::Subtract,
    BlendMode::LinearBurn,
    BlendMode::SoftLight,
    BlendMode::ColorDodge,
    BlendMode::ColorBurn,
    BlendMode::Hue,
    BlendMode::Saturation,
    BlendMode::Color,
    BlendMode::Luminosity,
];

const XFADE_STEP: f32 = 0.05;

pub fn apply(action: &Action, state: &mut SharedState, lib: &SceneLibrary) -> Result<()> {
    // Note: last_action_label is set by main.rs::handle_action before
    // dispatching here, so early-return paths (TapTempo, Trigger,
    // preset-mode slot save/recall) still surface in the status panel.
    match action {
        Action::AdvanceMode => {
            state.active_mode = match state.active_mode {
                Mode::Scene => Mode::Param,
                Mode::Param => Mode::Look,
                Mode::Look => Mode::Scene,
            };
        }
        Action::ToggleLayer => state.active_layer = state.active_layer.other(),
        Action::Slot { n, other_layer } => {
            let layer = if *other_layer {
                state.active_layer.other()
            } else {
                state.active_layer
            };
            apply_slot(state, lib, layer, *n)?;
        }
        Action::XfadeMinus => {
            state.xfade = (state.xfade - XFADE_STEP).max(0.0);
            state.look_dirty = true;
        }
        Action::XfadePlus => {
            state.xfade = (state.xfade + XFADE_STEP).min(1.0);
            state.look_dirty = true;
        }
        Action::ParamMinus => apply_param_step(state, -1)?,
        Action::ParamPlus => apply_param_step(state, 1)?,
        Action::ParamAudioCycle { dir } => apply_param_audio_cycle(state, *dir)?,
        Action::ResetAllParams => {
            let layer = state.active_layer;
            let scene_name = match layer {
                Layer::A => state.layer_a.scene_name.clone(),
                Layer::B => state.layer_b.scene_name.clone(),
            };
            let scene = lib.require(&scene_name)?;
            let pm = crate::scene::ParamMap::from_scene(&scene.meta);
            match layer {
                Layer::A => state.layer_a.params = pm,
                Layer::B => state.layer_b.params = pm,
            }
            state.look_dirty = true;
        }
        Action::BlendCycle => {
            let cur_idx = BLEND_MODES
                .iter()
                .position(|m| *m == state.blend_mode)
                .unwrap_or(0);
            state.blend_mode = BLEND_MODES[(cur_idx + 1) % BLEND_MODES.len()];
            state.look_dirty = true;
        }
        Action::Trigger => state.trigger = 1.0,
        Action::FreezeToggle => state.freeze_active = !state.freeze_active,
        Action::TapTempo => { /* handled by tap-tempo subsystem in Task 24 */ }
        Action::AudioBypass => {
            state.audio_bypass = !state.audio_bypass;
            state.look_dirty = true;
        }
        Action::Panic => {
            state.layer_a.scene_name = SAFE_SCENE_NAME.to_string();
            state.layer_b.scene_name = SAFE_SCENE_NAME.to_string();
            state.xfade = 0.5;
            state.audio_bypass = true;
            state.active_mode = Mode::Scene;
        }
        Action::ReloadAllScenes => { /* handled by hot_reload caller in main */ }
        Action::SceneCycle { layer, dir } => apply_scene_cycle(state, lib, *layer, *dir)?,
        Action::SceneCycleActive { dir } => {
            let layer = state.active_layer;
            apply_scene_cycle(state, lib, layer, *dir)?;
        }
        Action::SceneCycleOther { dir } => {
            let layer = state.active_layer.other();
            apply_scene_cycle(state, lib, layer, *dir)?;
        }
        Action::SetBlendMode(bm) => state.blend_mode = *bm,
        Action::SetXfade(v) => state.xfade = v.clamp(0.0, 1.0),
        Action::SetSceneByIndex { layer, index } => {
            let names: Vec<String> = lib
                .names()
                .filter(|n| !n.starts_with("__"))
                .map(|s| s.to_string())
                .collect();
            if let Some(name) = names.get(*index as usize) {
                set_scene(state, lib, *layer, name)?;
            }
        }
        Action::SetSceneByName { layer, name } => {
            set_scene(state, lib, *layer, name)?;
        }
        Action::RecallLook { .. } | Action::SaveLook { .. } => {
            // Handled by caller with LookStore. Apply layer is purely state-mutating
            // and doesn't own the looks file.
        }
        Action::DebugOverlayToggle => {
            state.status_overlay_visible = !state.status_overlay_visible;
        }
        Action::OpenMenu(_) => {
            // Stack manipulation lives in main; apply has no state to mutate
            // here. Kept exhaustive so adding a new menu kind forces a thought.
        }
    }
    Ok(())
}

pub const SAFE_SCENE_NAME: &str = "__safe__";

fn set_scene(state: &mut SharedState, lib: &SceneLibrary, layer: Layer, name: &str) -> Result<()> {
    let scene = lib.require(name)?;
    let new_state = crate::state::LayerState {
        scene_name: name.to_string(),
        params: crate::scene::ParamMap::from_scene(&scene.meta),
    };
    match layer {
        Layer::A => state.layer_a = new_state,
        Layer::B => state.layer_b = new_state,
    }
    state.look_dirty = true;
    Ok(())
}

fn apply_slot(state: &mut SharedState, lib: &SceneLibrary, layer: Layer, n: u8) -> Result<()> {
    match state.active_mode {
        Mode::Scene => {
            if (1..=9).contains(&n) {
                let names: Vec<String> = lib
                    .names()
                    .filter(|nm| !nm.starts_with("__"))
                    .map(|s| s.to_string())
                    .collect();
                if let Some(name) = resolve_slot(&state.slot_bindings, &names, n) {
                    let name = name.to_string();
                    return apply(&Action::SetSceneByName { layer, name }, state, lib);
                }
            }
        }
        Mode::Param => {
            if (1..=9).contains(&n) {
                match layer {
                    Layer::A => state.selected_param_a = n - 1,
                    Layer::B => state.selected_param_b = n - 1,
                }
            }
        }
        Mode::Look => {
            if n == 9 {
                let action = Action::ResetAllParams;
                return apply(&action, state, lib);
            }
            // Look 1-8 handled by caller (main.rs) with access to LookStore.
            // No-op here.
        }
    }
    Ok(())
}

fn apply_param_step(state: &mut SharedState, dir: i8) -> Result<()> {
    if matches!(state.active_mode, Mode::Param) {
        let layer = state.active_layer;
        let slot = state.selected_param();
        let layer_state = match layer {
            Layer::A => &mut state.layer_a,
            Layer::B => &mut state.layer_b,
        };
        let defs = layer_state.params.defs().to_vec();
        if let Some(def) = defs.iter().find(|d| d.slot == slot) {
            let cur = layer_state.params.get(&def.name).unwrap_or(def.default);
            let step = (def.max - def.min) * 0.02;
            let new_val = cur + step * dir as f32;
            layer_state.params.set(&def.name, new_val);
            state.look_dirty = true;
        }
    } else {
        // In Scene/Look modes, -/+ are crossfade nudges instead.
        if dir < 0 {
            state.xfade = (state.xfade - XFADE_STEP).max(0.0);
        } else {
            state.xfade = (state.xfade + XFADE_STEP).min(1.0);
        }
        state.look_dirty = true;
    }
    Ok(())
}

fn apply_param_audio_cycle(state: &mut SharedState, dir: i8) -> Result<()> {
    if !matches!(state.active_mode, Mode::Param) {
        return Ok(());
    }
    let layer = state.active_layer;
    let slot = state.selected_param();
    let layer_state = match layer {
        Layer::A => &mut state.layer_a,
        Layer::B => &mut state.layer_b,
    };
    let name = layer_state
        .params
        .defs()
        .iter()
        .find(|d| d.slot == slot)
        .map(|d| d.name.clone());
    if let Some(n) = name {
        layer_state.params.cycle_audio_route(&n, dir);
        state.look_dirty = true;
    }
    Ok(())
}

fn apply_scene_cycle(
    state: &mut SharedState,
    lib: &SceneLibrary,
    layer: Layer,
    dir: i8,
) -> Result<()> {
    let names: Vec<String> = lib
        .names()
        .filter(|n| !n.starts_with("__"))
        .map(|s| s.to_string())
        .collect();
    if names.is_empty() {
        return Ok(());
    }
    let cur_name = match layer {
        Layer::A => &state.layer_a.scene_name,
        Layer::B => &state.layer_b.scene_name,
    };
    let cur_idx = names.iter().position(|n| n == cur_name).unwrap_or(0) as i32;
    let next_idx = ((cur_idx + dir as i32).rem_euclid(names.len() as i32)) as u8;
    let action = Action::SetSceneByIndex {
        layer,
        index: next_idx,
    };
    apply(&action, state, lib)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{LoadedScene, SceneMeta};
    use crate::state::{BlendMode, SharedState};

    fn lib_with(scenes: &[(&str, &str)]) -> SceneLibrary {
        let mut lib = SceneLibrary::default();
        for (name, params_toml) in scenes {
            let s = format!("name = \"{}\"\n{}", name, params_toml);
            let meta = SceneMeta::parse(&s, "inline").unwrap();
            lib.upsert(
                name,
                LoadedScene {
                    meta,
                    fragment_body: "void main() {}".into(),
                    source_path: std::path::PathBuf::from("inline"),
                },
            );
        }
        lib
    }

    fn base_state(lib: &SceneLibrary) -> SharedState {
        SharedState::from_initial(lib, "alpha", "beta", 0.0, BlendMode::Mix).unwrap()
    }

    fn three_scenes() -> SceneLibrary {
        let p = "[[params]]\nslot = 0\nname = \"x\"\nmin = 0.0\nmax = 1.0\ndefault = 0.5\n";
        lib_with(&[("alpha", p), ("beta", p), ("gamma", p)])
    }

    #[test]
    fn advance_mode_cycles_three_modes() {
        let lib = three_scenes();
        let mut s = base_state(&lib);
        apply(&Action::AdvanceMode, &mut s, &lib).unwrap();
        assert_eq!(s.active_mode, Mode::Param);
        apply(&Action::AdvanceMode, &mut s, &lib).unwrap();
        assert_eq!(s.active_mode, Mode::Look);
        apply(&Action::AdvanceMode, &mut s, &lib).unwrap();
        assert_eq!(s.active_mode, Mode::Scene);
    }

    #[test]
    fn toggle_layer_swaps() {
        let lib = three_scenes();
        let mut s = base_state(&lib);
        assert_eq!(s.active_layer, Layer::A);
        apply(&Action::ToggleLayer, &mut s, &lib).unwrap();
        assert_eq!(s.active_layer, Layer::B);
    }

    #[test]
    fn slot_in_scene_mode_uses_explicit_binding_over_alphabetical() {
        let lib = three_scenes(); // alpha, beta, gamma alphabetical
        let mut s = base_state(&lib);
        s.slot_bindings.set(1, Some("gamma".into()));
        apply(
            &Action::Slot {
                n: 1,
                other_layer: false,
            },
            &mut s,
            &lib,
        )
        .unwrap();
        // Slot 1 would have been "alpha" by alphabetical fallback, but the
        // explicit binding wins.
        assert_eq!(s.layer_a.scene_name, "gamma");
    }

    #[test]
    fn slot_in_scene_mode_falls_back_to_alphabetical_when_binding_missing() {
        let lib = three_scenes();
        let mut s = base_state(&lib);
        apply(
            &Action::Slot {
                n: 2,
                other_layer: false,
            },
            &mut s,
            &lib,
        )
        .unwrap();
        assert_eq!(s.layer_a.scene_name, "beta");
    }

    #[test]
    fn slot_in_scene_mode_sets_scene_by_index() {
        let lib = three_scenes(); // alphabetical: alpha, beta, gamma
        let mut s = base_state(&lib);
        apply(
            &Action::Slot {
                n: 3,
                other_layer: false,
            },
            &mut s,
            &lib,
        )
        .unwrap();
        assert_eq!(s.layer_a.scene_name, "gamma");
    }

    #[test]
    fn slot_with_other_layer_targets_b() {
        let lib = three_scenes();
        let mut s = base_state(&lib);
        apply(
            &Action::Slot {
                n: 2,
                other_layer: true,
            },
            &mut s,
            &lib,
        )
        .unwrap();
        assert_eq!(s.layer_b.scene_name, "beta");
        assert_eq!(s.layer_a.scene_name, "alpha"); // unchanged
    }

    #[test]
    fn xfade_clamps() {
        let lib = three_scenes();
        let mut s = base_state(&lib);
        for _ in 0..50 {
            apply(&Action::XfadePlus, &mut s, &lib).unwrap();
        }
        assert_eq!(s.xfade, 1.0);
        for _ in 0..50 {
            apply(&Action::XfadeMinus, &mut s, &lib).unwrap();
        }
        assert_eq!(s.xfade, 0.0);
    }

    #[test]
    fn param_plus_in_param_mode_steps_selected_slot() {
        let lib = three_scenes();
        let mut s = base_state(&lib);
        s.active_mode = Mode::Param;
        s.selected_param_a = 0;
        let before = s.layer_a.params.get("x").unwrap();
        apply(&Action::ParamPlus, &mut s, &lib).unwrap();
        let after = s.layer_a.params.get("x").unwrap();
        assert!(after > before, "{after} should be > {before}");
        assert!(s.look_dirty);
    }

    #[test]
    fn slot_9_in_param_mode_selects_ninth_slot() {
        let lib = three_scenes();
        let mut s = base_state(&lib);
        s.active_mode = Mode::Param;
        apply(
            &Action::Slot {
                n: 9,
                other_layer: false,
            },
            &mut s,
            &lib,
        )
        .unwrap();
        assert_eq!(s.selected_param_a, 8);
    }

    #[test]
    fn slot_9_in_preset_mode_resets_all_params() {
        let lib = three_scenes();
        let mut s = base_state(&lib);
        s.active_mode = Mode::Look;
        s.layer_a.params.set("x", 0.99);
        apply(
            &Action::Slot {
                n: 9,
                other_layer: false,
            },
            &mut s,
            &lib,
        )
        .unwrap();
        assert_eq!(s.layer_a.params.get("x"), Some(0.5)); // back to default
    }

    #[test]
    fn blend_cycle_wraps_after_full_cycle() {
        let lib = three_scenes();
        let mut s = base_state(&lib);
        for _ in 0..19 {
            apply(&Action::BlendCycle, &mut s, &lib).unwrap();
        }
        assert_eq!(s.blend_mode, BlendMode::Mix);
    }

    #[test]
    fn blend_cycle_visits_overlay_after_difference() {
        let lib = three_scenes();
        let mut s = base_state(&lib);
        for _ in 0..5 {
            apply(&Action::BlendCycle, &mut s, &lib).unwrap();
        }
        assert_eq!(s.blend_mode, BlendMode::Overlay);
    }

    #[test]
    fn blend_cycle_visits_softlight_after_linearburn() {
        let lib = three_scenes();
        let mut s = base_state(&lib);
        for _ in 0..12 {
            apply(&Action::BlendCycle, &mut s, &lib).unwrap();
        }
        assert_eq!(s.blend_mode, BlendMode::SoftLight);
    }

    #[test]
    fn panic_swaps_to_safe_scene() {
        let lib = three_scenes();
        let mut s = base_state(&lib);
        apply(&Action::Panic, &mut s, &lib).unwrap();
        assert_eq!(s.layer_a.scene_name, SAFE_SCENE_NAME);
        assert_eq!(s.layer_b.scene_name, SAFE_SCENE_NAME);
        assert_eq!(s.xfade, 0.5);
        assert!(s.audio_bypass);
    }

    #[test]
    fn freeze_toggles() {
        let lib = three_scenes();
        let mut s = base_state(&lib);
        apply(&Action::FreezeToggle, &mut s, &lib).unwrap();
        assert!(s.freeze_active);
        apply(&Action::FreezeToggle, &mut s, &lib).unwrap();
        assert!(!s.freeze_active);
    }
}
