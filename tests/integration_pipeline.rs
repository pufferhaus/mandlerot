//! Integration smoke tests — opens a real GL context, compiles every shipped
//! scene, renders 60 frames, asserts no errors and at least some non-black
//! output.

#![cfg(feature = "desktop")]

use mandlerot::headless::HeadlessRun;
use mandlerot::scene::SceneLibrary;
use mandlerot::state::BlendMode;
use std::path::PathBuf;

fn scenes_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scenes")
}

/// Requires a display server (opens a real GL window via winit).
/// Run locally with: cargo test --test integration_pipeline -- --ignored
#[test]
#[ignore = "Requires display; run locally with `--ignored`"]
fn every_scene_renders_60_frames() {
    let lib = SceneLibrary::load_dir(&scenes_dir()).expect("load scenes");
    let names: Vec<String> = lib.names().map(|s| s.to_string()).collect();
    assert!(names.len() >= 3, "expected at least 3 scenes, got {names:?}");

    for name in &names {
        let run = HeadlessRun {
            frames: 60,
            scene_a: name.clone(),
            scene_b: "solid".into(),
            xfade: 0.0, // see only A
            blend_mode: BlendMode::Mix,
            width: 256,
            height: 256,
            dump_to: None,
        };
        let captures = run.run(&lib).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(captures.len(), 60, "{name}: frame count");
        // At xfade=0, output should equal layer A. For non-degenerate scenes,
        // some pixel must be non-black on at least one frame.
        let any_lit = captures.iter().any(|frame| frame.iter().any(|&b| b > 8));
        assert!(any_lit, "{name}: every frame entirely black");
    }
}

/// Requires a display server (opens a real GL window via winit).
/// Run locally with: cargo test --test integration_pipeline -- --ignored
#[test]
#[ignore = "Requires display; run locally with `--ignored`"]
fn xfade_at_one_shows_layer_b_only() {
    let lib = SceneLibrary::load_dir(&scenes_dir()).unwrap();
    // A=solid red (1,0,0), B=solid blue (0,0,1), xfade=1 → expect mostly blue
    let run_a = HeadlessRun {
        frames: 1,
        scene_a: "solid".into(),
        scene_b: "solid".into(),
        xfade: 0.0,
        blend_mode: BlendMode::Mix,
        width: 64,
        height: 64,
        dump_to: None,
    };
    let _ = run_a.run(&lib).unwrap(); // warmup ensures driver is sane
    let run_b = HeadlessRun {
        frames: 1,
        scene_a: "solid".into(),
        scene_b: "plasma".into(),
        xfade: 1.0,
        blend_mode: BlendMode::Mix,
        width: 64,
        height: 64,
        dump_to: None,
    };
    let frames = run_b.run(&lib).unwrap();
    // With xfade=1.0 the output is layer B only. Plasma is colorful; assert
    // that at least one pixel has nonzero green or blue (solid red wouldn't).
    let frame = &frames[0];
    let any_chromatic = frame.chunks_exact(4).any(|px| px[1] > 32 || px[2] > 32);
    assert!(any_chromatic, "xfade=1 should show plasma, not red");
}
