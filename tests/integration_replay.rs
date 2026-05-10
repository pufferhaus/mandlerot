//! Drives `apply` with a `MockInput` script through the keymap and asserts
//! end state.

use std::path::PathBuf;
use std::time::Duration;

use mandlerot::action::Action;
use mandlerot::apply::apply;
use mandlerot::input::keymap::{KeyMap, Modifier};
use mandlerot::input::mock::MockInput;
use mandlerot::scene::{LoadedScene, SceneLibrary, SceneMeta};
use mandlerot::state::{BlendMode, Mode, SharedState};

fn fixture_lib() -> SceneLibrary {
    let mut lib = SceneLibrary::default();
    for n in ["alpha", "beta", "gamma"] {
        let s = format!(
            "name = \"{n}\"\n[[params]]\nslot = 0\nname = \"x\"\nmin = 0.0\nmax = 1.0\ndefault = 0.5\n"
        );
        let meta = SceneMeta::parse(&s, n).unwrap();
        lib.upsert(
            n,
            LoadedScene {
                meta,
                fragment_body: "void main() {}".into(),
                source_path: PathBuf::from(n),
            },
        );
    }
    lib
}

#[test]
fn replay_quickset_drives_state() {
    let lib = fixture_lib();
    let mut state = SharedState::from_initial(&lib, "alpha", "alpha", 0.0, BlendMode::Mix).unwrap();
    let km = KeyMap::parse(include_str!("../keymap.toml")).unwrap();
    let mut input =
        MockInput::from_script(include_str!("fixtures/replay_quickset.txt")).unwrap();

    // Drain the whole script.
    let events = input.drain_until(Duration::from_secs(10));

    for (key, modifier) in events {
        if let Some(action) = km.lookup(&key, modifier, &state) {
            apply(&action, &mut state, &lib).unwrap();
        }
    }

    // After "press 2": layer_a should be 'beta' (alphabetical index 1)
    // After "press Tab": mode = Param
    // After "press 1": selected_param_a = 0
    // After 5x "press =" in PARAM mode: param 'x' incremented 5 steps from default
    // After 2x "press =" still in PARAM mode: 7 increments total

    assert_eq!(state.layer_a.scene_name, "beta");
    assert_eq!(state.active_mode, Mode::Param);
    assert_eq!(state.selected_param_a, 0);
    let cur = state.layer_a.params.get("x").unwrap();
    let expected = 0.5 + 7.0 * 0.02; // 7 increments × 2% of range
    assert!((cur - expected).abs() < 1e-4, "got {cur}, expected ~{expected}");
}
