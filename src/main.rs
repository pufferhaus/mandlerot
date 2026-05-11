use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(all(feature = "pi", target_os = "linux"))]
struct NullBackend;
#[cfg(all(feature = "pi", target_os = "linux"))]
impl mandlerot::status::Backend for NullBackend {
    fn flush_full(&mut self, _fb: &mandlerot::status::render::Fb) -> mandlerot::Result<()> {
        Ok(())
    }
    fn flush_runs(
        &mut self,
        _fb: &mandlerot::status::render::Fb,
        _runs: &[(usize, usize, usize)],
    ) -> mandlerot::Result<()> {
        Ok(())
    }
}

use anyhow::Context;
use clap::Parser;

use mandlerot::action::{Action, MenuKind};
use mandlerot::apply::apply;
use mandlerot::audio::params::AudioParams;
use mandlerot::audio::thread::{spawn as spawn_audio, AtomicAudio};
use mandlerot::config::Config;
use mandlerot::hot_reload::{HotReloader, ReloadEvent};
use mandlerot::input::double_tap::DoubleTap;
use mandlerot::input::keymap::{KeyMap, Modifier};
use mandlerot::input::mock::MockInput;
use mandlerot::preset::{LookStore, SlotBindings};
use mandlerot::ui::{RenderCtx, ScreenCtx, ScreenStack};
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
    /// Path to the persistent looks/preset JSON. Defaults to `looks.json`
    /// in the working directory; if missing, a legacy `presets.json` will
    /// be migrated in place on first run.
    #[arg(long, default_value = "looks.json", alias = "presets")]
    looks: PathBuf,
    /// Headless smoke: render N frames and exit.
    #[arg(long)]
    smoke_frames: Option<u32>,
    /// Replay a scripted input file (no audio capture).
    #[arg(long)]
    replay: Option<PathBuf>,
    /// Open a separate window showing the SPI status panel preview (desktop only).
    #[arg(long)]
    status_window: bool,
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
    // One-time migration: rename legacy `presets.json` to the new `looks.json`
    // location if the new file is missing. Idempotent — runs every startup
    // but only acts when the conditions are exactly right.
    if !cli.looks.exists() {
        let legacy = std::path::PathBuf::from("presets.json");
        if legacy.exists() {
            if let Err(e) = std::fs::rename(&legacy, &cli.looks) {
                tracing::warn!("migrate presets.json → {:?} failed: {e}", cli.looks);
            } else {
                tracing::info!("migrated presets.json → {:?}", cli.looks);
            }
        }
    }
    let mut looks = LookStore::load_or_empty(&cli.looks).context("load looks")?;
    let blend_mode = BlendMode::parse(&cfg.initial.blend_mode).unwrap_or(BlendMode::Mix);
    let mut state = SharedState::from_initial(
        &library,
        &cfg.initial.scene_a,
        &cfg.initial.scene_b,
        cfg.initial.xfade,
        blend_mode,
    )?;
    let state_dir = mandlerot::config::user_state_dir();
    state.slot_bindings = SlotBindings::load_or_empty(&state_dir);
    let audio_params = AudioParams::load_or_default(&state_dir);
    let mut ui_stack = ScreenStack::new();
    // Double-tap Esc/Backspace is the unconditional escape hatch: even
    // with a menu open (which normally swallows Esc), two quick taps
    // close the menu *and* fire Panic.
    let mut esc_double = DoubleTap::new(400);

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
    // Compile the baked safe-scene up front so PANIC always has a working
    // fallback even if the user's scene compile breaks.
    pipeline.upsert_scene("__safe__", library.require("__safe__")?)?;
    pipeline.upsert_scene(
        &state.layer_a.scene_name,
        library.require(&state.layer_a.scene_name)?,
    )?;
    pipeline.upsert_scene(
        &state.layer_b.scene_name,
        library.require(&state.layer_b.scene_name)?,
    )?;

    let watcher = HotReloader::watch(&cli.scenes).context("hot watcher")?;

    let audio_atomic = Arc::new(AtomicAudio::new());
    let audio_stop = Arc::new(AtomicBool::new(false));
    let audio_history = mandlerot::audio::history::AudioHistory::new();
    let _audio_thread = if cli.replay.is_none() {
        Some(spawn_audio(
            audio_atomic.clone(),
            audio_params.clone(),
            audio_stop.clone(),
        ))
    } else {
        None
    };

    // Status panel — backend selection and optional live preview window.
    let status_backend: Box<dyn mandlerot::status::Backend> = {
        #[cfg(all(feature = "pi", target_os = "linux"))]
        {
            match mandlerot::status::pi::PiPanelBackend::open() {
                Ok(b) => Box::new(b) as Box<_>,
                Err(e) => {
                    tracing::warn!("SPI panel unavailable: {e}; running without status panel");
                    Box::new(NullBackend)
                }
            }
        }
        #[cfg(not(all(feature = "pi", target_os = "linux")))]
        {
            if cli.status_window {
                let be = mandlerot::status::desktop::DesktopBufferBackend::new();
                // Hand a clone of the shared buffer to the render target so it
                // can open (and paint) the second preview window.
                target.enable_status_window(be.buf.clone());
                Box::new(be) as Box<_>
            } else {
                Box::new(mandlerot::status::desktop::DesktopPngBackend::new(
                    "target/status.png",
                )) as Box<_>
            }
        }
    };
    let (status_handle, _status_join) =
        mandlerot::status::thread::spawn(status_backend, library.clone());

    let mut supervisor = mandlerot::supervisor::Supervisor::new();
    #[cfg(target_os = "linux")]
    let mut watchdog = mandlerot::watchdog::Watchdog::open("/dev/watchdog").ok();
    let mut tap_tempo = TapTempo::new();
    #[cfg(all(feature = "desktop", not(feature = "pi")))]
    let mut input_winit = mandlerot::input::winit_src::WinitInputState::default();
    #[cfg(all(feature = "pi", target_os = "linux"))]
    let mut input_evdev = match mandlerot::input::evdev_src::EvdevInput::open_all() {
        Ok(e) => Some(e),
        Err(e) => {
            tracing::warn!("evdev: {e}; running without HID input");
            None
        }
    };
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

        #[cfg(target_os = "linux")]
        if let Some(w) = watchdog.as_mut() {
            w.pet();
        }

        // Hot-reload
        while let Some(evt) = watcher.try_recv() {
            handle_reload(evt, &cli.scenes, &mut library, &mut pipeline);
        }

        // Audio sample
        let bands = audio_atomic.load_bands();
        let beat_value = audio_atomic.load_beat();
        state.audio_bands = bands;
        // Record this frame's bands into the rolling history ring and push
        // the resulting RGBA snapshot to the GPU. Sampling at the render
        // rate (not the audio rate) preserves the pre-existing semantics of
        // "1 row per frame" that the spectrogram_waterfall scene relied on
        // when it was scrolling u_prev. ~1.3 KB/frame upload is trivial.
        audio_history.push(bands);
        let history_bytes = audio_history.snapshot_rgba();
        pipeline.upload_audio_history(&history_bytes);
        // Trigger sourcing:
        //   - bypassed: audio is silent; decay the manual Action::Trigger pulse
        //     here so visual reactivity to a tap fades naturally.
        //   - live:     the BeatDetector already owns its own decay; sample it
        //     directly. A manual Action::Trigger fired this same frame still
        //     reads as 1.0 in the shader (apply runs before render); next
        //     frame it's overwritten by the audio value, matching the
        //     "one-frame pulse" semantic documented on Action::Trigger.
        if state.audio_bypass {
            state.trigger *= 0.85;
        } else {
            state.trigger = beat_value;
        }

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
            #[cfg(all(feature = "pi", target_os = "linux"))]
            if let Some(evdev) = input_evdev.as_mut() {
                for pair in evdev.poll() {
                    events.push(pair);
                }
            }
        }
        for (key, modifier) in events {
            // Double-tap Esc/Backspace fires Panic *and* closes any open
            // menu — runs before stack dispatch so menus can't swallow it.
            if matches!(key.as_str(), "Esc" | "Backspace")
                && esc_double.tap(Instant::now())
            {
                ui_stack.close_all();
                handle_action(
                    &Action::Panic,
                    &mut state,
                    &library,
                    &mut looks,
                    &mut tap_tempo,
                );
                state.last_action_label = "Panic (double-esc)".to_string();
                continue;
            }
            // While a menu screen is open, all key events go to it. The menu
            // operates on bindings (persistent) and never touches the live
            // shader state, so the running visuals stay in sync regardless.
            if ui_stack.is_open() {
                let scene_names: Vec<String> = library
                    .names()
                    .filter(|n| !n.starts_with("__"))
                    .map(|s| s.to_string())
                    .collect();
                let mut ctx = ScreenCtx {
                    scenes: &scene_names,
                    bindings: &mut state.slot_bindings,
                    state_dir: &state_dir,
                    audio: &audio_params,
                };
                ui_stack.handle_key(&key, &mut ctx);
                continue;
            }
            if let Some(action) = keymap.lookup(&key, modifier, &state) {
                // F4 (or any OpenMenu) is intercepted here so we don't push
                // the action through `apply` — the menu lives entirely on
                // the main thread alongside the screen stack.
                if let Action::OpenMenu(kind) = &action {
                    match kind {
                        MenuKind::Settings => ui_stack.open(Box::new(
                            mandlerot::ui::screens::SettingsScreen::new(),
                        )),
                    }
                    state.last_action_label = format!("OpenMenu({kind:?})");
                    continue;
                }
                let old_a = state.layer_a.scene_name.clone();
                let old_b = state.layer_b.scene_name.clone();
                handle_action(&action, &mut state, &library, &mut looks, &mut tap_tempo);
                if state.layer_a.scene_name != old_a {
                    supervisor.enable(&state.layer_a.scene_name);
                }
                if state.layer_b.scene_name != old_b {
                    supervisor.enable(&state.layer_b.scene_name);
                }
            }
        }

        // Time
        if !state.freeze_active {
            state.time_secs = start.elapsed().as_secs_f32();
        }
        // Trigger decay is owned by the audio thread (BeatDetector); we just
        // sample its current value above. We do NOT decay state.trigger here:
        // doing so on top of the audio-thread decay would compound rates and
        // shorten manual `Action::Trigger` pulses unpredictably.

        // Lazy-compile any scene referenced by state but not yet in the pipeline.
        // Actions like SetSceneByIndex / scene cycle / preset recall change
        // `scene_name` without compiling; do it here so the next render finds
        // a program. Compile failures are recorded in the supervisor; after
        // MAX_FAULTS the scene is auto-disabled and substituted with __safe__.
        for layer_name in [
            state.layer_a.scene_name.clone(),
            state.layer_b.scene_name.clone(),
        ] {
            if pipeline.has_scene(&layer_name) {
                continue;
            }
            match library.require(&layer_name) {
                Ok(scene) => {
                    if let Err(e) = pipeline.upsert_scene(&layer_name, scene) {
                        tracing::warn!("compile {layer_name}: {e}; recording fault");
                        if supervisor.record_fault(&layer_name) {
                            tracing::warn!("scene {layer_name} disabled after repeated faults");
                        }
                    }
                }
                Err(_) => {
                    if supervisor.record_fault(&layer_name) {
                        tracing::warn!("scene {layer_name} disabled (not in library)");
                    }
                }
            }
        }
        // Substitute disabled scenes with __safe__.
        let resolved_a = supervisor.resolve(&state.layer_a.scene_name).to_string();
        if resolved_a != state.layer_a.scene_name {
            state.layer_a.scene_name = resolved_a;
        }
        let resolved_b = supervisor.resolve(&state.layer_b.scene_name).to_string();
        if resolved_b != state.layer_b.scene_name {
            state.layer_b.scene_name = resolved_b;
        }

        // If a menu is open, paint it on the main thread (where bindings and
        // scene names live) and send the rendered grid; the status worker
        // blits it verbatim instead of composing from state.
        let menu_grid = if ui_stack.is_open() {
            let scene_names: Vec<String> = library
                .names()
                .filter(|n| !n.starts_with("__"))
                .map(|s| s.to_string())
                .collect();
            let rctx = RenderCtx {
                scenes: &scene_names,
                bindings: &state.slot_bindings,
                audio: &audio_params,
            };
            ui_stack.render_top(&rctx)
        } else {
            None
        };
        status_handle.try_send(mandlerot::status::thread::StateSnapshot {
            state: state.clone(),
            menu_grid,
        });

        if let Err(e) = pipeline.frame(&state, target.dimensions().0, target.dimensions().1) {
            tracing::error!("frame error: {e}");
        }
        if state.status_overlay_visible {
            let strip_text = mandlerot::overlay::build_strip_text(&state);
            let rgba = mandlerot::overlay::rasterize(&strip_text);
            let (tw, th) = target.dimensions();
            pipeline.draw_overlay_strip(
                &rgba,
                mandlerot::overlay::STRIP_W,
                mandlerot::overlay::STRIP_H,
                4,
                4,
                tw,
                th,
            );
        }
        target.present()?;
        #[cfg(all(feature = "desktop", not(feature = "pi")))]
        target.paint_status();
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
    status_handle.stop.store(true, Ordering::Relaxed);
    Ok(())
}

fn handle_action(
    action: &Action,
    state: &mut SharedState,
    lib: &SceneLibrary,
    looks: &mut LookStore,
    tap: &mut TapTempo,
) {
    // Set the status panel's "LAST" label up front so early-return paths
    // (TapTempo, Trigger, preset-mode slot save/recall) still surface in
    // the UI. apply() used to do this internally; centralizing here avoids
    // the gaps.
    state.last_action_label = format!("{:?}", action);
    match action {
        Action::TapTempo => {
            state.tap_tempo_bpm = tap.tap(Instant::now());
            return;
        }
        Action::Trigger => {
            state.trigger = 1.0;
            return;
        }
        // Look save/recall: in LOOK mode, slot 1-8 with other_layer=true → save,
        //                                   slot 1-8 with other_layer=false → recall.
        Action::Slot { n, other_layer }
            if state.active_mode == Mode::Look && (1..=8).contains(n) =>
        {
            let slot = *n;
            if *other_layer {
                if let Err(e) = looks.save(slot, state, None) {
                    tracing::warn!("preset save: {e}");
                } else {
                    state.active_look_slot = Some(slot);
                    state.look_dirty = false;
                }
            } else if let Err(e) = looks.recall(slot, state, lib) {
                tracing::warn!("preset recall: {e}");
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
