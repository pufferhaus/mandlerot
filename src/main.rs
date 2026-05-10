use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::Parser;

use mandlerot::action::Action;
use mandlerot::apply::apply;
use mandlerot::audio::thread::{spawn as spawn_audio, AtomicAudio};
use mandlerot::config::Config;
use mandlerot::hot_reload::{HotReloader, ReloadEvent};
use mandlerot::input::keymap::{KeyMap, Modifier};
use mandlerot::input::mock::MockInput;
use mandlerot::preset::PresetStore;
use mandlerot::render::pipeline::Pipeline;
use mandlerot::render::target::RenderTarget;
use mandlerot::scene::{LoadedScene, SceneLibrary, SceneMeta};
use mandlerot::state::{BlendMode, Mode, SharedState};
use mandlerot::tap_tempo::TapTempo;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    #[arg(long, default_value = "config.toml")]
    config: PathBuf,
    #[arg(long, default_value = "scenes")]
    scenes: PathBuf,
    #[arg(long, default_value = "keymap.toml")]
    keymap: PathBuf,
    #[arg(long, default_value = "presets.json")]
    presets: PathBuf,
    /// Headless smoke: render N frames and exit.
    #[arg(long)]
    smoke_frames: Option<u32>,
    /// Replay a scripted input file (no audio capture).
    #[arg(long)]
    replay: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let cli = Cli::parse();

    let cfg = Config::load(&cli.config).context("load config")?;
    let mut library = SceneLibrary::load_dir(&cli.scenes).context("load scenes")?;
    let keymap = KeyMap::load(&cli.keymap).context("load keymap")?;
    let mut presets = PresetStore::load_or_empty(&cli.presets).context("load presets")?;
    let blend_mode = BlendMode::parse(&cfg.initial.blend_mode).unwrap_or(BlendMode::Mix);
    let mut state = SharedState::from_initial(
        &library,
        &cfg.initial.scene_a,
        &cfg.initial.scene_b,
        cfg.initial.xfade,
        blend_mode,
    )?;

    #[cfg(all(feature = "desktop", not(feature = "pi")))]
    let mut target = {
        use mandlerot::render::desktop::WinitGlTarget;
        WinitGlTarget::new(cfg.render.width, cfg.render.height, "mandleROT")?
    };
    #[cfg(all(feature = "pi", target_os = "linux"))]
    let mut target = {
        use mandlerot::render::pi::PiTarget;
        PiTarget::new(cfg.render.width, cfg.render.height)?
    };

    let gl: Arc<glow::Context> = target.gl();
    let mut pipeline = Pipeline::new(gl, cfg.render.width, cfg.render.height)?;
    pipeline.upsert_scene(&state.layer_a.scene_name, library.require(&state.layer_a.scene_name)?)?;
    pipeline.upsert_scene(&state.layer_b.scene_name, library.require(&state.layer_b.scene_name)?)?;

    let watcher = HotReloader::watch(&cli.scenes).context("hot watcher")?;

    let audio_atomic = Arc::new(AtomicAudio::new());
    let audio_stop = Arc::new(AtomicBool::new(false));
    let _audio_thread = if cli.replay.is_none() {
        Some(spawn_audio(audio_atomic.clone(), audio_stop.clone()))
    } else {
        None
    };

    let mut tap_tempo = TapTempo::new();
    #[cfg(all(feature = "desktop", not(feature = "pi")))]
    let mut input_winit = mandlerot::input::winit_src::WinitInputState::default();
    let mut mock_input = if let Some(path) = &cli.replay {
        let s = std::fs::read_to_string(path).context("read replay file")?;
        Some(MockInput::from_script(&s)?)
    } else {
        None
    };

    let start = Instant::now();
    let frame_dt = Duration::from_micros(1_000_000 / cfg.render.fps as u64);
    let mut frame = 0u64;

    loop {
        let frame_start = Instant::now();

        // Hot-reload
        while let Some(evt) = watcher.try_recv() {
            handle_reload(evt, &cli.scenes, &mut library, &mut pipeline);
        }

        // Audio sample
        let bands = audio_atomic.load_bands();
        let beat_value = audio_atomic.load_beat();
        state.audio_bands = bands;
        // u_trigger = max(beat decay, manual trigger pulse)
        state.trigger = state.trigger.max(beat_value);

        // Input
        let mut events: Vec<(String, Modifier)> = Vec::new();
        if let Some(mock) = mock_input.as_mut() {
            let now = Instant::now().duration_since(start);
            for evt in mock.drain_until(now) {
                events.push(evt);
            }
        } else {
            #[cfg(all(feature = "desktop", not(feature = "pi")))]
            for ev in target.drain_key_events() {
                if let Some(pair) = input_winit.handle(&ev) {
                    events.push(pair);
                }
            }
        }
        for (key, modifier) in events {
            if let Some(action) = keymap.lookup(&key, modifier, &state) {
                handle_action(&action, &mut state, &library, &mut presets, &mut tap_tempo);
            }
        }

        // Time
        if !state.freeze_active {
            state.time_secs = start.elapsed().as_secs_f32();
        }
        // Trigger decay (~150 ms half-life)
        state.trigger *= 0.85;

        if let Err(e) = pipeline.frame(&state, target.dimensions().0, target.dimensions().1) {
            tracing::error!("frame error: {e}");
        }
        target.present()?;
        if !target.pump() {
            break;
        }
        if let Some(n) = cli.smoke_frames {
            if frame as u32 + 1 >= n {
                break;
            }
        }
        if let Some(mock) = mock_input.as_ref() {
            if mock.finished() {
                tracing::info!("replay finished");
                break;
            }
        }
        frame += 1;

        let elapsed = frame_start.elapsed();
        if elapsed < frame_dt {
            std::thread::sleep(frame_dt - elapsed);
        }
    }

    audio_stop.store(true, Ordering::Relaxed);
    Ok(())
}

fn handle_action(
    action: &Action,
    state: &mut SharedState,
    lib: &SceneLibrary,
    presets: &mut PresetStore,
    tap: &mut TapTempo,
) {
    match action {
        Action::TapTempo => {
            state.tap_tempo_bpm = tap.tap(Instant::now());
            return;
        }
        Action::Trigger => {
            state.trigger = 1.0;
            return;
        }
        // Preset save/recall: in PRESET mode, slot 1-8 with other_layer=true → save,
        //                                   slot 1-8 with other_layer=false → recall.
        Action::Slot { n, other_layer } if state.active_mode == Mode::Preset && (1..=8).contains(n) => {
            let slot = *n;
            if *other_layer {
                if let Err(e) = presets.save(slot, state, None) {
                    tracing::warn!("preset save: {e}");
                } else {
                    state.active_preset_slot = Some(slot);
                    state.preset_dirty = false;
                }
            } else {
                if let Err(e) = presets.recall(slot, state, lib) {
                    tracing::warn!("preset recall: {e}");
                }
            }
            return;
        }
        _ => {}
    }
    if let Err(e) = apply(action, state, lib) {
        tracing::warn!("apply: {e}");
    }
}

fn handle_reload(
    evt: ReloadEvent,
    scenes_dir: &std::path::Path,
    library: &mut SceneLibrary,
    pipeline: &mut Pipeline,
) {
    let stem = match &evt {
        ReloadEvent::SceneTouched { stem } | ReloadEvent::SceneRemoved { stem } => stem.clone(),
    };
    if matches!(evt, ReloadEvent::SceneRemoved { .. }) {
        return;
    }
    let glsl_path = scenes_dir.join(format!("{stem}.glsl"));
    let toml_path = scenes_dir.join(format!("{stem}.toml"));
    if !glsl_path.exists() || !toml_path.exists() {
        return;
    }
    let body = match std::fs::read_to_string(&glsl_path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("read {glsl_path:?}: {e}");
            return;
        }
    };
    let meta_str = match std::fs::read_to_string(&toml_path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("read {toml_path:?}: {e}");
            return;
        }
    };
    let meta = match SceneMeta::parse(&meta_str, &toml_path.display().to_string()) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("scene meta: {e}");
            return;
        }
    };
    if let Err(e) = meta.validate() {
        tracing::warn!("scene validate: {e}");
        return;
    }
    let was_known = pipeline.has_scene(&stem);
    let scene = LoadedScene {
        meta,
        fragment_body: body,
        source_path: glsl_path,
    };
    library.upsert(&stem, scene.clone());
    match pipeline.upsert_scene(&stem, &scene) {
        Ok(()) => {
            if was_known {
                tracing::info!("hot-reloaded {stem}");
            } else {
                tracing::info!("loaded new scene {stem}");
            }
        }
        Err(e) => tracing::warn!("recompile {stem}: {e}"),
    }
}
