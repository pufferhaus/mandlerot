//! Integration harness: inject key strings into screens and assert on
//! ScreenResult / state mutations. No GL context required — PostFx screens
//! that early-return on `postfx: None` are noted in their sections.
//!
//! Run with: `cargo test --lib ui::screen_harness`

use super::{Screen, ScreenCtx, ScreenEvent, ScreenResult, ScreenStack};
use crate::audio::params::AudioParams;
use crate::preset::{LookStore, SlotBindings};
use crate::render::postfx::{
    snapshot_passes, tests_fake_pass, PostFxController, PostFxPass,
};
use crate::scene::{LoadedScene, ParamMap, SceneLibrary, SceneMeta};
use crate::state::{BlendMode, SharedState};
use crate::ui::screens::{
    AudioSettingsScreen, ChromakeyScreen, LooksScreen, PostFxScreen, SettingsScreen, SlotsScreen,
};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// FakePostFx — in-memory PostFxController for harness tests
// ---------------------------------------------------------------------------

struct FakePostFx {
    passes: Vec<PostFxPass>,
}

impl FakePostFx {
    fn with_passes(passes: Vec<PostFxPass>) -> Self {
        Self { passes }
    }
}

impl PostFxController for FakePostFx {
    fn passes(&self) -> &[PostFxPass] {
        &self.passes
    }

    fn toggle(&mut self, idx: usize) {
        if let Some(p) = self.passes.get_mut(idx) {
            p.enabled = !p.enabled;
        }
    }

    fn pass_params_mut(&mut self, idx: usize) -> Option<&mut ParamMap> {
        self.passes.get_mut(idx).map(|p| &mut p.params)
    }

    fn save_state(&self, _dir: &std::path::Path) -> crate::Result<()> {
        Ok(())
    }

    fn snapshot(&self) -> crate::preset::store::PostFxSnapshot {
        snapshot_passes(&self.passes)
    }

    fn apply_snapshot(&mut self, snap: &crate::preset::store::PostFxSnapshot) {
        crate::render::postfx::apply_snapshot_to_passes(&mut self.passes, snap);
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct ScreenHarness {
    stack: ScreenStack,
    state: SharedState,
    looks: LookStore,
    lib: SceneLibrary,
    audio: Arc<AudioParams>,
    tmp: TempDir,
}

fn build_lib() -> SceneLibrary {
    let mut lib = SceneLibrary::default();
    for n in ["alpha", "beta", "gamma", "delta", "epsilon"] {
        let meta = SceneMeta::parse(&format!("name = \"{n}\"\n"), "inline").unwrap();
        lib.upsert(
            n,
            LoadedScene {
                meta,
                fragment_body: "void main() {}".into(),
                source_path: PathBuf::from("inline"),
            },
        );
    }
    lib
}

// Free function so press() can split self.state / self.looks / self.stack
// as disjoint field borrows rather than borrowing all of self.
fn make_ctx<'a>(
    state: &'a mut SharedState,
    looks: &'a mut LookStore,
    audio: &'a Arc<AudioParams>,
    state_dir: &'a std::path::Path,
    scene_names: &'a [String],
) -> ScreenCtx<'a> {
    ScreenCtx {
        scenes: scene_names,
        bindings: &mut state.slot_bindings,
        state_dir,
        audio,
        postfx: None,
        chromakey: Some(&mut state.chromakey),
        video_status: crate::video::VideoStatus::NoDevice,
        active_look_slot: state.active_look_slot,
        looks: Some(looks),
    }
}

impl ScreenHarness {
    fn new() -> Self {
        let lib = build_lib();
        let state =
            SharedState::from_initial(&lib, "alpha", "beta", 0.0, BlendMode::Mix).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let looks = LookStore::load_or_empty(&tmp.path().join("looks.json")).unwrap();
        let audio = AudioParams::new();
        ScreenHarness { stack: ScreenStack::new(), state, looks, lib, audio, tmp }
    }

    fn push(&mut self, screen: Box<dyn Screen>) {
        self.stack.open(screen);
    }

    /// Inject one key string (exactly as the input layer delivers it, after
    /// any modifier handling). The ScreenStack's numpad translation runs
    /// automatically — tests for Numpad6→Up etc. use the raw numpad names.
    fn press(&mut self, key: &str) -> Option<ScreenEvent> {
        let scene_names: Vec<String> = self.lib.names().map(|s| s.to_string()).collect();
        let mut ctx = make_ctx(
            &mut self.state,
            &mut self.looks,
            &self.audio,
            self.tmp.path(),
            &scene_names,
        );
        self.stack.handle_key(key, &mut ctx);
        self.stack.take_pending()
    }

    fn press_n(&mut self, key: &str, n: usize) {
        for _ in 0..n {
            self.press(key);
        }
    }

    fn depth(&self) -> usize { self.stack.depth() }
    fn is_open(&self) -> bool { self.stack.is_open() }

    /// Like `press`, but provides a `&mut dyn PostFxController` in the
    /// ScreenCtx. Used by PostFxScreen tests that need a live chain.
    fn press_with_postfx(
        &mut self,
        key: &str,
        pfx: &mut dyn PostFxController,
    ) -> Option<ScreenEvent> {
        let scene_names: Vec<String> = self.lib.names().map(|s| s.to_string()).collect();
        let mut ctx = ScreenCtx {
            scenes: &scene_names,
            bindings: &mut self.state.slot_bindings,
            state_dir: self.tmp.path(),
            audio: &self.audio,
            postfx: Some(pfx),
            chromakey: Some(&mut self.state.chromakey),
            video_status: crate::video::VideoStatus::NoDevice,
            active_look_slot: self.state.active_look_slot,
            looks: Some(&mut self.looks),
        };
        self.stack.handle_key(key, &mut ctx);
        self.stack.take_pending()
    }
}

// ---------------------------------------------------------------------------
// LooksScreen
// ---------------------------------------------------------------------------

#[test]
fn looks_esc_pops() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    h.press("Esc");
    assert!(!h.is_open());
}

#[test]
fn looks_backspace_pops() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    h.press("Backspace");
    assert!(!h.is_open());
}

#[test]
fn looks_enter_emits_recall_slot_1_at_top() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    let ev = h.press("Enter");
    assert_eq!(ev, Some(ScreenEvent::RecallLook(1)));
}

#[test]
fn looks_numpadenter_emits_recall() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    let ev = h.press("NumpadEnter");
    assert_eq!(ev, Some(ScreenEvent::RecallLook(1)));
}

#[test]
fn looks_down_then_enter_emits_recall_slot_2() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    h.press("Down");
    let ev = h.press("Enter");
    assert_eq!(ev, Some(ScreenEvent::RecallLook(2)));
}

#[test]
fn looks_each_slot_reachable_by_down() {
    for target in 1u8..=8 {
        let mut h = ScreenHarness::new();
        h.push(Box::new(LooksScreen::new()));
        h.press_n("Down", (target - 1) as usize);
        let ev = h.press("Enter");
        assert_eq!(ev, Some(ScreenEvent::RecallLook(target)), "slot {target}");
    }
}

#[test]
fn looks_cursor_clamps_at_slot_8() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    h.press_n("Down", 20);
    let ev = h.press("Enter");
    assert_eq!(ev, Some(ScreenEvent::RecallLook(8)));
}

#[test]
fn looks_cursor_clamps_at_top() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    h.press_n("Up", 10);
    let ev = h.press("Enter");
    assert_eq!(ev, Some(ScreenEvent::RecallLook(1)));
}

#[test]
fn looks_up_after_down_returns_to_slot_1() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    h.press("Down");
    h.press("Up");
    let ev = h.press("Enter");
    assert_eq!(ev, Some(ScreenEvent::RecallLook(1)));
}

#[test]
fn looks_d_emits_delete_slot_1() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    let ev = h.press("d");
    assert_eq!(ev, Some(ScreenEvent::DeleteLook(1)));
}

#[test]
fn looks_shift_d_emits_delete() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    let ev = h.press("D");
    assert_eq!(ev, Some(ScreenEvent::DeleteLook(1)));
}

#[test]
fn looks_delete_key_emits_delete() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    let ev = h.press("Delete");
    assert_eq!(ev, Some(ScreenEvent::DeleteLook(1)));
}

#[test]
fn looks_down_then_d_deletes_slot_2() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    h.press("Down");
    let ev = h.press("d");
    assert_eq!(ev, Some(ScreenEvent::DeleteLook(2)));
}

// Numpad translation: Numpad4→Down, Numpad6→Up (translated by ScreenStack).
#[test]
fn looks_numpad4_acts_as_down() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    h.press("Numpad4"); // → Down
    let ev = h.press("Enter");
    assert_eq!(ev, Some(ScreenEvent::RecallLook(2)));
}

#[test]
fn looks_numpad6_acts_as_up() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    h.press_n("Numpad4", 3); // → Down ×3 → cursor at 3
    h.press("Numpad6");       // → Up → cursor at 2
    let ev = h.press("Enter");
    assert_eq!(ev, Some(ScreenEvent::RecallLook(3)));
}

// ---------------------------------------------------------------------------
// SettingsScreen
// ---------------------------------------------------------------------------
// Entries: 0=Preferences(stub), 1=Audio, 2=Slots, 3=Post-FX, 4=Chromakey

#[test]
fn settings_esc_pops() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(SettingsScreen::new()));
    h.press("Esc");
    assert!(!h.is_open());
}

#[test]
fn settings_preferences_enter_does_not_push() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(SettingsScreen::new()));
    h.press("Enter"); // row 0 = Preferences stub
    assert_eq!(h.depth(), 1);
}

#[test]
fn settings_each_real_entry_pushes_a_screen() {
    for row in 1u8..=4 {
        let mut h = ScreenHarness::new();
        h.push(Box::new(SettingsScreen::new()));
        h.press_n("Down", row as usize);
        h.press("Enter");
        assert_eq!(h.depth(), 2, "row {row} should push a child screen");
        h.press("Esc");
        assert_eq!(h.depth(), 1, "child screen Esc should pop back");
    }
}

#[test]
fn settings_digit_1_does_not_push_preferences() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(SettingsScreen::new()));
    h.press("1"); // entry 0 = Preferences stub
    assert_eq!(h.depth(), 1);
}

#[test]
fn settings_digit_2_pushes_audio() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(SettingsScreen::new()));
    h.press("2"); // Audio
    assert_eq!(h.depth(), 2);
}

#[test]
fn settings_digit_3_pushes_slots() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(SettingsScreen::new()));
    h.press("3"); // Slot Mapper
    assert_eq!(h.depth(), 2);
}

#[test]
fn settings_digit_5_pushes_chromakey() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(SettingsScreen::new()));
    h.press("5"); // Chromakey
    assert_eq!(h.depth(), 2);
}

#[test]
fn settings_cursor_clamps_at_last_entry() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(SettingsScreen::new()));
    h.press_n("Down", 20);
    h.press("Enter"); // should still push Chromakey (last entry, idx 4)
    assert_eq!(h.depth(), 2);
}

#[test]
fn settings_numpad_add_acts_as_down() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(SettingsScreen::new()));
    // NumpadAdd → Down → entry 1 (Audio) → Enter → depth 2
    h.press("NumpadAdd");
    h.press("Enter");
    assert_eq!(h.depth(), 2);
}

#[test]
fn settings_numpad_subtract_acts_as_up() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(SettingsScreen::new()));
    h.press_n("Down", 3); // cursor at 3 (Post-FX)
    h.press("NumpadSubtract"); // → Up → cursor at 2 (Slots)
    h.press("Enter");
    assert_eq!(h.depth(), 2); // Slots screen pushed
}

#[test]
fn settings_numlock_acts_as_up() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(SettingsScreen::new()));
    h.press_n("Down", 2);
    h.press("NumLock"); // → Up → back to 1
    h.press("Enter");
    assert_eq!(h.depth(), 2);
}

// ---------------------------------------------------------------------------
// SlotsScreen
// ---------------------------------------------------------------------------

#[test]
fn slots_esc_pops() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(SlotsScreen::new()));
    h.press("Esc");
    assert!(!h.is_open());
}

#[test]
fn slots_enter_pushes_scene_list() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(SlotsScreen::new()));
    h.press("Enter");
    assert_eq!(h.depth(), 2);
}

#[test]
fn slots_scene_list_esc_pops_back() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(SlotsScreen::new()));
    h.press("Enter");
    assert_eq!(h.depth(), 2);
    h.press("Esc");
    assert_eq!(h.depth(), 1);
}

#[test]
fn slots_digit_jumps_and_pushes_scene_list() {
    for slot in 1u8..=9 {
        let mut h = ScreenHarness::new();
        h.push(Box::new(SlotsScreen::new()));
        h.press(&slot.to_string());
        assert_eq!(h.depth(), 2, "digit {slot} should push SceneListScreen");
        h.press("Esc");
        assert_eq!(h.depth(), 1);
    }
}

#[test]
fn slots_zero_clears_binding_without_pushing() {
    let mut h = ScreenHarness::new();
    // Set a binding first
    h.state.slot_bindings.set(1, Some("alpha".to_string()));
    h.push(Box::new(SlotsScreen::new()));
    h.press("0");
    assert_eq!(h.depth(), 1); // no push
    assert_eq!(h.state.slot_bindings.get(1), None);
}

#[test]
fn slots_cursor_clamps_at_slot_9() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(SlotsScreen::new()));
    h.press_n("Down", 20); // clamped at 8 (slot 9)
    h.press("Enter");
    assert_eq!(h.depth(), 2); // SceneListScreen for slot 9 pushed
}

#[test]
fn slots_scene_list_enter_binds_and_pops() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(SlotsScreen::new()));
    h.press("Enter"); // open SceneListScreen for slot 1
    assert_eq!(h.depth(), 2);
    h.press("Enter"); // select scene → binds + pop
    assert_eq!(h.depth(), 1); // back at SlotsScreen
    assert!(h.state.slot_bindings.get(1).is_some());
}

// ---------------------------------------------------------------------------
// AudioSettingsScreen
// ---------------------------------------------------------------------------
// Rows: 0=NoiseFloor, 1=Bass, 2=LoMid, 3=HiMid, 4=Treble, 5=Device(Enter→push)

#[test]
fn audio_esc_pops() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(AudioSettingsScreen::new()));
    h.press("Esc");
    assert!(!h.is_open());
}

#[test]
fn audio_right_nudges_noise_floor_up() {
    let mut h = ScreenHarness::new();
    let before = h.audio.noise_floor();
    h.push(Box::new(AudioSettingsScreen::new()));
    h.press("Right");
    assert!(h.audio.noise_floor() > before, "Right should increase noise floor");
}

#[test]
fn audio_left_nudges_noise_floor_down() {
    let mut h = ScreenHarness::new();
    let before = h.audio.noise_floor();
    h.push(Box::new(AudioSettingsScreen::new()));
    h.press("Left");
    assert!(h.audio.noise_floor() < before, "Left should decrease noise floor");
}

#[test]
fn audio_r_resets_noise_floor_to_default() {
    use crate::audio::params::DEFAULT_NOISE_FLOOR;
    let mut h = ScreenHarness::new();
    h.push(Box::new(AudioSettingsScreen::new()));
    h.press_n("Right", 5); // nudge away from default
    h.press("r");
    assert!(
        (h.audio.noise_floor() - DEFAULT_NOISE_FLOOR).abs() < 1e-4,
        "r should reset to default"
    );
}

#[test]
fn audio_numpad8_acts_as_left() {
    let mut h = ScreenHarness::new();
    let before = h.audio.noise_floor();
    h.push(Box::new(AudioSettingsScreen::new()));
    h.press("Numpad8"); // → Left
    assert!(h.audio.noise_floor() < before);
}

#[test]
fn audio_numpad2_acts_as_right() {
    let mut h = ScreenHarness::new();
    let before = h.audio.noise_floor();
    h.push(Box::new(AudioSettingsScreen::new()));
    h.press("Numpad2"); // → Right
    assert!(h.audio.noise_floor() > before);
}

#[test]
fn audio_down_to_gain_bass_then_nudge() {
    let mut h = ScreenHarness::new();
    let before = h.audio.gain(0); // bass = band 0
    h.push(Box::new(AudioSettingsScreen::new()));
    h.press("Down"); // cursor → bass row
    h.press("Right");
    assert!(h.audio.gain(0) > before);
}

#[test]
fn audio_enter_on_device_row_pushes_device_screen() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(AudioSettingsScreen::new()));
    h.press_n("Down", 6); // cursor → device row (past 6 knobs)
    h.press("Enter");
    assert_eq!(h.depth(), 2);
}

#[test]
fn audio_left_right_on_device_row_no_crash() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(AudioSettingsScreen::new()));
    h.press_n("Down", 6);
    h.press("Left");
    h.press("Right");
    assert_eq!(h.depth(), 1); // no push, no pop
}

// ---------------------------------------------------------------------------
// ChromakeyScreen
// ---------------------------------------------------------------------------
// Rows: 0=Enabled, 1=Color(cycle), 2=Luma(Left/Right), 3=Soft(Left/Right), 4=Spill

#[test]
fn chromakey_esc_pops() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(ChromakeyScreen::new()));
    h.press("Esc");
    assert!(!h.is_open());
}

#[test]
fn chromakey_space_on_enabled_row_toggles() {
    let mut h = ScreenHarness::new();
    let before = h.state.chromakey.enabled;
    h.push(Box::new(ChromakeyScreen::new()));
    h.press("Space");
    assert_ne!(h.state.chromakey.enabled, before);
    h.press("Space");
    assert_eq!(h.state.chromakey.enabled, before);
}

#[test]
fn chromakey_enter_on_enabled_row_toggles() {
    let mut h = ScreenHarness::new();
    let before = h.state.chromakey.enabled;
    h.push(Box::new(ChromakeyScreen::new()));
    h.press("Enter");
    assert_ne!(h.state.chromakey.enabled, before);
}

#[test]
fn chromakey_space_on_color_row_cycles_preset() {
    let mut h = ScreenHarness::new();
    let before = h.state.chromakey.key_color;
    h.push(Box::new(ChromakeyScreen::new()));
    h.press("Down"); // → row 1 (Color)
    h.press("Space");
    assert_ne!(h.state.chromakey.key_color, before, "preset should cycle");
}

#[test]
fn chromakey_right_on_luma_row_nudges_up() {
    let mut h = ScreenHarness::new();
    let before = h.state.chromakey.luma_threshold;
    h.push(Box::new(ChromakeyScreen::new()));
    h.press_n("Down", 2); // → row 2 (Luma)
    h.press("Right");
    assert!(h.state.chromakey.luma_threshold > before);
}

#[test]
fn chromakey_left_on_luma_row_nudges_down() {
    let mut h = ScreenHarness::new();
    // Start with a non-zero value so there's room to decrease.
    h.state.chromakey.luma_threshold = 0.1;
    let before = h.state.chromakey.luma_threshold;
    h.push(Box::new(ChromakeyScreen::new()));
    h.press_n("Down", 2);
    h.press("Left");
    assert!(h.state.chromakey.luma_threshold < before);
}

#[test]
fn chromakey_luma_clamps_at_zero() {
    let mut h = ScreenHarness::new();
    h.state.chromakey.luma_threshold = 0.0;
    h.push(Box::new(ChromakeyScreen::new()));
    h.press_n("Down", 2);
    h.press_n("Left", 20);
    assert_eq!(h.state.chromakey.luma_threshold, 0.0);
}

#[test]
fn chromakey_right_on_soft_row_nudges_up() {
    let mut h = ScreenHarness::new();
    let before = h.state.chromakey.edge_soft;
    h.push(Box::new(ChromakeyScreen::new()));
    h.press_n("Down", 3); // → row 3 (Soft edge)
    h.press("Right");
    assert!(h.state.chromakey.edge_soft > before);
}

#[test]
fn chromakey_space_on_spill_row_toggles_spill() {
    let mut h = ScreenHarness::new();
    let before = h.state.chromakey.spill_suppress;
    h.push(Box::new(ChromakeyScreen::new()));
    h.press_n("Down", 4); // → row 4 (Spill)
    h.press("Space");
    assert_ne!(h.state.chromakey.spill_suppress, before);
}

#[test]
fn chromakey_cursor_clamps_at_last_row() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(ChromakeyScreen::new()));
    h.press_n("Down", 20); // clamps at row 4 (Spill)
    let before = h.state.chromakey.spill_suppress;
    h.press("Space");
    assert_ne!(h.state.chromakey.spill_suppress, before);
}

#[test]
fn chromakey_numpad8_acts_as_left_on_luma() {
    let mut h = ScreenHarness::new();
    h.state.chromakey.luma_threshold = 0.1;
    let before = h.state.chromakey.luma_threshold;
    h.push(Box::new(ChromakeyScreen::new()));
    h.press_n("Down", 2);
    h.press("Numpad8"); // → Left
    assert!(h.state.chromakey.luma_threshold < before);
}

#[test]
fn chromakey_numpad2_acts_as_right_on_luma() {
    let mut h = ScreenHarness::new();
    let before = h.state.chromakey.luma_threshold;
    h.push(Box::new(ChromakeyScreen::new()));
    h.press_n("Down", 2);
    h.press("Numpad2"); // → Right
    assert!(h.state.chromakey.luma_threshold > before);
}

// ---------------------------------------------------------------------------
// PostFxScreen — navigation only (postfx: None causes early-return Pop)
// ---------------------------------------------------------------------------

#[test]
fn postfx_any_key_pops_when_postfx_unavailable() {
    // PostFxScreen::handle_key early-returns Pop when ctx.postfx is None.
    // This verifies graceful degradation rather than a crash.
    let mut h = ScreenHarness::new();
    h.push(Box::new(PostFxScreen::new()));
    assert_eq!(h.depth(), 1);
    h.press("Down");
    assert_eq!(h.depth(), 0); // popped immediately
}

#[test]
fn postfx_esc_pops_when_postfx_unavailable() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(PostFxScreen::new()));
    h.press("Esc");
    assert!(!h.is_open());
}

// ---------------------------------------------------------------------------
// Stack-level numpad translation (spot-checks via LooksScreen)
// ---------------------------------------------------------------------------

#[test]
fn stack_numpad6_translates_to_up_before_screen_sees_it() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    h.press_n("Down", 3); // cursor at 3 → slot 4
    h.press("Numpad6");   // → Up → cursor at 2 → slot 3
    let ev = h.press("Enter");
    assert_eq!(ev, Some(ScreenEvent::RecallLook(3)));
}

#[test]
fn stack_numpad4_translates_to_down_before_screen_sees_it() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    h.press("Numpad4"); // → Down → cursor at 1 → slot 2
    let ev = h.press("Enter");
    assert_eq!(ev, Some(ScreenEvent::RecallLook(2)));
}

// ---------------------------------------------------------------------------
// Render assertions
// ---------------------------------------------------------------------------

impl ScreenHarness {
    fn render_top(&self) -> Option<crate::status::TextScreen> {
        let scene_names: Vec<String> = self.lib.names().map(|s| s.to_string()).collect();
        let rctx = crate::ui::RenderCtx {
            scenes: &scene_names,
            bindings: &self.state.slot_bindings,
            audio: &self.audio,
            postfx: None,
            chromakey: Some(&self.state.chromakey),
            filtered_scenes: 0,
            pi_gen: crate::platform::PiGen::Unknown,
            video_status: crate::video::VideoStatus::NoDevice,
            active_look_slot: self.state.active_look_slot,
            bound_state: None,
            looks_view: None,
        };
        self.stack.render_top(&rctx)
    }

    fn render_top_with_looks(&self) -> Option<crate::status::TextScreen> {
        let scene_names: Vec<String> = self.lib.names().map(|s| s.to_string()).collect();
        let looks_view: [Option<(&str, &str)>; 8] = std::array::from_fn(|i| {
            let key = (i + 1).to_string();
            self.looks.file.slots.get(&key).map(|l| (l.name.as_str(), l.saved_at.as_str()))
        });
        let rctx = crate::ui::RenderCtx {
            scenes: &scene_names,
            bindings: &self.state.slot_bindings,
            audio: &self.audio,
            postfx: None,
            chromakey: Some(&self.state.chromakey),
            filtered_scenes: 0,
            pi_gen: crate::platform::PiGen::Unknown,
            video_status: crate::video::VideoStatus::NoDevice,
            active_look_slot: self.state.active_look_slot,
            bound_state: None,
            looks_view: Some(&looks_view),
        };
        self.stack.render_top(&rctx)
    }
}

fn read_str(grid: &crate::status::TextScreen, row: usize, col: usize, len: usize) -> String {
    (col..col + len).map(|c| grid.at(row, c).ch).collect()
}

// LooksScreen render tests

#[test]
fn looks_render_title_in_border() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    let grid = h.render_top_with_looks().unwrap();
    let title = read_str(&grid, 0, 3, 7);
    assert_eq!(title, " LOOKS ");
}

#[test]
fn looks_render_cursor_on_first_slot() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    let grid = h.render_top_with_looks().unwrap();
    assert_eq!(grid.at(4, 2).ch, '>');
    assert_ne!(grid.at(6, 2).ch, '>');
}

#[test]
fn looks_render_slot_numbers_1_through_8() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    let grid = h.render_top_with_looks().unwrap();
    for slot in 1u8..=8 {
        let row = 4 + ((slot - 1) as usize) * 2;
        let expected = char::from_digit(slot as u32, 10).unwrap();
        assert_eq!(grid.at(row, 4).ch, expected, "slot {slot} number wrong");
    }
}

#[test]
fn looks_render_empty_slots_show_empty_label() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    let grid = h.render_top_with_looks().unwrap();
    let label = read_str(&grid, 4, 7, 7);
    assert_eq!(label, "(empty)");
}

#[test]
fn looks_render_saved_slot_shows_name() {
    let mut h = ScreenHarness::new();
    h.looks.save(1, &h.state, Some("ubik-warp".into())).unwrap();
    h.push(Box::new(LooksScreen::new()));
    let grid = h.render_top_with_looks().unwrap();
    let name = read_str(&grid, 4, 7, 9);
    assert_eq!(name, "ubik-warp");
}

#[test]
fn looks_render_active_slot_marker() {
    let mut h = ScreenHarness::new();
    h.looks.save(2, &h.state, Some("pkd-warp".into())).unwrap();
    h.state.active_look_slot = Some(2);
    h.push(Box::new(LooksScreen::new()));
    let grid = h.render_top_with_looks().unwrap();
    assert_eq!(grid.at(6, 5).ch, '*');
    assert_ne!(grid.at(4, 5).ch, '*');
}

#[test]
fn looks_render_cursor_moves_after_down() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(LooksScreen::new()));
    h.press("Down");
    let grid = h.render_top_with_looks().unwrap();
    assert_eq!(grid.at(6, 2).ch, '>');
    assert_ne!(grid.at(4, 2).ch, '>');
}

// SettingsScreen render tests

#[test]
fn settings_render_title_in_border() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(SettingsScreen::new()));
    let grid = h.render_top().unwrap();
    let title = read_str(&grid, 0, 3, 10);
    assert_eq!(title, " SETTINGS ");
}

#[test]
fn settings_render_audio_entry_label() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(SettingsScreen::new()));
    let grid = h.render_top().unwrap();
    // Audio is entry index 1, start_row=5, so row = 5 + 1*2 = 7, label at col 7
    let label = read_str(&grid, 7, 7, 5);
    assert_eq!(label, "Audio");
}

#[test]
fn settings_render_slot_mapper_label() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(SettingsScreen::new()));
    let grid = h.render_top().unwrap();
    // Slot Mapper is entry index 2, row = 5 + 2*2 = 9, label at col 7
    let label = read_str(&grid, 9, 7, 11);
    assert_eq!(label, "Slot Mapper");
}

// ChromakeyScreen render tests

#[test]
fn chromakey_render_title_in_border() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(ChromakeyScreen::new()));
    let grid = h.render_top().unwrap();
    let title = read_str(&grid, 0, 3, 11);
    assert_eq!(title, " CHROMAKEY ");
}

#[test]
fn chromakey_render_enabled_value_unchecked() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(ChromakeyScreen::new()));
    let grid = h.render_top().unwrap();
    // Enabled row: row = 4 + 0*2 = 4, value at col 24
    let val = read_str(&grid, 4, 24, 3);
    assert_eq!(val, "[ ]");
}

#[test]
fn chromakey_render_enabled_value_after_toggle() {
    let mut h = ScreenHarness::new();
    h.push(Box::new(ChromakeyScreen::new()));
    h.press("Space");
    let grid = h.render_top().unwrap();
    let val = read_str(&grid, 4, 24, 3);
    assert_eq!(val, "[x]");
}

// ---------------------------------------------------------------------------
// PostFxScreen (with FakePostFx)
// ---------------------------------------------------------------------------

#[test]
fn postfx_esc_pops_with_passes() {
    let mut h = ScreenHarness::new();
    let mut pfx = FakePostFx::with_passes(vec![
        tests_fake_pass("vignette", true, &[]),
        tests_fake_pass("grain", false, &[]),
    ]);
    h.push(Box::new(PostFxScreen::new()));
    h.press_with_postfx("Esc", &mut pfx);
    assert!(!h.is_open());
}

#[test]
fn postfx_space_toggles_pass() {
    let mut h = ScreenHarness::new();
    let mut pfx = FakePostFx::with_passes(vec![
        tests_fake_pass("vignette", true, &[]),
    ]);
    h.push(Box::new(PostFxScreen::new()));
    assert!(pfx.passes()[0].enabled);
    h.press_with_postfx("Space", &mut pfx);
    assert!(!pfx.passes()[0].enabled);
    h.press_with_postfx("Space", &mut pfx);
    assert!(pfx.passes()[0].enabled);
}

#[test]
fn postfx_down_moves_cursor_between_passes() {
    let mut h = ScreenHarness::new();
    let mut pfx = FakePostFx::with_passes(vec![
        tests_fake_pass("vignette", true, &[]),
        tests_fake_pass("grain", false, &[]),
    ]);
    h.push(Box::new(PostFxScreen::new()));
    // Cursor starts at 0 (vignette). Space toggles vignette.
    h.press_with_postfx("Space", &mut pfx);
    assert!(!pfx.passes()[0].enabled);
    // Move down to grain, Space toggles grain.
    h.press_with_postfx("Down", &mut pfx);
    h.press_with_postfx("Space", &mut pfx);
    assert!(pfx.passes()[1].enabled);
}

#[test]
fn postfx_right_pushes_param_screen() {
    let mut h = ScreenHarness::new();
    let mut pfx = FakePostFx::with_passes(vec![
        tests_fake_pass("vignette", true, &[("intensity", 0.5)]),
    ]);
    h.push(Box::new(PostFxScreen::new()));
    assert_eq!(h.depth(), 1);
    h.press_with_postfx("Right", &mut pfx);
    assert_eq!(h.depth(), 2); // PostFxParamScreen pushed
}

#[test]
fn postfx_cursor_clamps_at_last_pass() {
    let mut h = ScreenHarness::new();
    let mut pfx = FakePostFx::with_passes(vec![
        tests_fake_pass("vignette", true, &[]),
        tests_fake_pass("grain", false, &[]),
    ]);
    h.push(Box::new(PostFxScreen::new()));
    // Down 20× then Space should toggle the last pass (grain at idx 1).
    for _ in 0..20 { h.press_with_postfx("Down", &mut pfx); }
    h.press_with_postfx("Space", &mut pfx);
    assert!(pfx.passes()[1].enabled);  // grain was false, now true
    assert!(pfx.passes()[0].enabled);  // vignette unchanged
}
