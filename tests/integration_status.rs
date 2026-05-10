//! Verifies the status grid composer produces stable output for a known state.

use std::path::PathBuf;

use mandlerot::scene::{LoadedScene, SceneLibrary, SceneMeta};
use mandlerot::state::{BlendMode, Layer, Mode, SharedState};
use mandlerot::status::compose::state_to_grid;
use mandlerot::status::grid::{ATTR_INVERSE, COLS, ROWS};

fn lib() -> SceneLibrary {
    let mut l = SceneLibrary::default();
    let m = SceneMeta::parse(
        "name = \"plasma\"\n[[params]]\nslot = 0\nname = \"hue\"\nmin = 0.0\nmax = 1.0\ndefault = 0.5\n",
        "x",
    )
    .unwrap();
    l.upsert(
        "plasma",
        LoadedScene {
            meta: m,
            fragment_body: "void main() {}".into(),
            source_path: PathBuf::from("inline"),
        },
    );
    let m2 = SceneMeta::parse(
        "name = \"solid\"\n[[params]]\nslot = 0\nname = \"red\"\nmin = 0.0\nmax = 1.0\ndefault = 1.0\n",
        "x",
    )
    .unwrap();
    l.upsert(
        "solid",
        LoadedScene {
            meta: m2,
            fragment_body: "void main() {}".into(),
            source_path: PathBuf::from("inline"),
        },
    );
    l
}

#[test]
fn grid_dimensions_are_80_by_26() {
    let s = SharedState::from_initial(&lib(), "plasma", "solid", 0.0, BlendMode::Mix).unwrap();
    let g = state_to_grid(&s, &lib());
    assert_eq!(g.cells.len(), COLS * ROWS);
}

#[test]
fn switching_layer_changes_active_marker() {
    let mut s = SharedState::from_initial(&lib(), "plasma", "solid", 0.0, BlendMode::Mix).unwrap();
    let g_a = state_to_grid(&s, &lib());
    s.active_layer = Layer::B;
    let g_b = state_to_grid(&s, &lib());
    assert_ne!(
        g_a.cells, g_b.cells,
        "switching layer must change at least one cell"
    );
}

#[test]
fn switching_mode_to_param_changes_top_bar() {
    let mut s = SharedState::from_initial(&lib(), "plasma", "solid", 0.0, BlendMode::Mix).unwrap();
    let g_scene = state_to_grid(&s, &lib());
    s.active_mode = Mode::Param;
    let g_param = state_to_grid(&s, &lib());
    let in_scene: String = (8..13).map(|c| g_scene.at(0, c).ch).collect();
    let in_param: String = (8..13).map(|c| g_param.at(0, c).ch).collect();
    assert!(in_scene.contains("SCENE"));
    assert!(in_param.contains("PARAM"));
}

#[test]
fn xfade_at_one_puts_marker_on_right() {
    let s = SharedState::from_initial(&lib(), "plasma", "solid", 1.0, BlendMode::Mix).unwrap();
    let g = state_to_grid(&s, &lib());
    let mut marker_cols = Vec::new();
    for col in 3..60 {
        if g.at(11, col).ch == '█' {
            marker_cols.push(col);
        }
    }
    assert_eq!(marker_cols.len(), 1);
    assert!(
        marker_cols[0] >= 55,
        "expected marker near right end, got col {}",
        marker_cols[0]
    );
}

#[test]
fn inverse_attr_used_on_active_mode_label() {
    let s = SharedState::from_initial(&lib(), "plasma", "solid", 0.0, BlendMode::Mix).unwrap();
    let g = state_to_grid(&s, &lib());
    // Mode label "SCENE" starts at col 8 in row 0.
    for c in 8..13 {
        assert_ne!(g.at(0, c).attr & ATTR_INVERSE, 0);
    }
}
