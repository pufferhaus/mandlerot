use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
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
use mandlerot::input::chord::ChordWatcher;
use mandlerot::input::double_tap::DoubleTap;
use mandlerot::input::keymap::{KeyMap, Modifier};
use mandlerot::input::mock::MockInput;
use mandlerot::preset::{LookStore, SlotBindings};
use mandlerot::ui::{RenderCtx, ScreenCtx, ScreenStack};
use mandlerot::render::pipeline::Pipeline;
use mandlerot::render::postfx::PostFx;
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
    /// Directory of post-FX pass pairs (`<name>.glsl` + `<name>.toml`).
    /// Missing dir = no chain. Phase-1 default ships Vignette+Grain on,
    /// Pixelate off.
    #[arg(long, default_value = "postfx")]
    postfx: PathBuf,
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
    /// Benchmark every loadable scene for N frames each, log fps + render time,
    /// and exit. Output: tracing logs at INFO level (one `bench:` line per scene).
    #[arg(long)]
    benchmark: Option<u32>,
    /// Demo mode: cycle through every scene for N seconds each at the normal
    /// 30fps display pace, then exit. Status panel + audio stay live; postfx
    /// is forced off so the visual output is the raw scene. Intended for
    /// verifying per-scene visual fidelity on the TV after tuning.
    #[arg(long)]
    demo_seconds: Option<u32>,
}

/// Benchmark helper. Runs a 30-frame warmup then a measurement window, calls
/// `report(fps, avg_render_ms)`. Kept generic over `RenderTarget` so the same
/// function works for both the Pi DRM target and the desktop winit target.
fn measure<T: RenderTarget, R: FnOnce(f64, f64)>(
    pipeline: &mut Pipeline,
    target: &mut T,
    state: &SharedState,
    scan: (u32, u32),
    frames: usize,
    report: R,
) -> anyhow::Result<()> {
    for _ in 0..30 {
        let _ = pipeline.frame(state, scan.0, scan.1);
        target.present()?;
    }
    let bench_start = Instant::now();
    let mut render_us_total: u64 = 0;
    for _ in 0..frames {
        let t = Instant::now();
        let _ = pipeline.frame(state, scan.0, scan.1);
        target.present()?;
        render_us_total += t.elapsed().as_micros() as u64;
    }
    let wall = bench_start.elapsed().as_secs_f64();
    let fps = frames as f64 / wall;
    let avg_ms = (render_us_total as f64 / frames as f64) / 1000.0;
    report(fps, avg_ms);
    Ok(())
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let cli = Cli::parse();

    let pi_gen = mandlerot::platform::detect();
    tracing::info!("detected platform: {:?}", pi_gen);

    let cfg = Config::load(&cli.config).context("load config")?;
    let mut library =
        SceneLibrary::load_dir_for_gen(&cli.scenes, pi_gen).context("load scenes")?;
    let filtered = library.filtered_count();
    if filtered > 0 {
        tracing::info!(
            "scene filter: {} scene(s) hidden (require gen above {:?})",
            filtered,
            pi_gen,
        );
    }
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
    state.chromakey = mandlerot::render::chromakey::ChromakeyState::load_or_default(&state_dir);
    let mut ui_stack = ScreenStack::new();
    // Double-tap Esc is the unconditional keyboard escape hatch: even with
    // a menu open (which normally swallows Esc), two quick taps close the
    // menu *and* fire Panic. (Backspace used to share this role but is now
    // bound to SceneCycleActive next on the numpad — two fast cycles would
    // have falsely fired Panic. Numpad operators reach Panic via the
    // `-+Enter` ChordWatcher below.)
    let mut esc_double = DoubleTap::new(400);
    // Numpad Panic chord: hitting `-`, `+`, and `Enter` within 400 ms (any
    // order) fires Panic. Same global escape semantics as the double-tap.
    let mut panic_chord = ChordWatcher::new(
        &["NumpadSubtract", "NumpadAdd", "NumpadEnter"],
        400,
    );

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
    // `MANDLEROT_RENDER_SCALE` is the per-host override written by the
    // install-time `pi-gen.conf` systemd drop-in (Pi 3 → 0.33, Pi 4 → 0.66,
    // Pi 5 → 1.0). It wins over `config.toml` so `make deploy` re-rsync
    // can't clobber the per-host tier. Garbage values fall through to the
    // shipped config value.
    let base_scale = cfg.render.render_scale;
    let scale = std::env::var("MANDLEROT_RENDER_SCALE")
        .ok()
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(base_scale)
        .clamp(0.25, 1.0);
    let (scan_w, scan_h) = target.dimensions();
    let render_w = ((scan_w as f32 * scale).round() as u32).max(64);
    let render_h = ((scan_h as f32 * scale).round() as u32).max(64);
    if scale < 0.999 {
        tracing::info!(
            "render scale {:.2}: internal {}x{} → scanout {}x{}",
            scale, render_w, render_h, scan_w, scan_h
        );
    }
    let mut pipeline = Pipeline::new_for_gen(gl, render_w, render_h, pi_gen)?;
    // Compile the baked safe-scene up front so PANIC always has a working
    // fallback even if the user's scene compile breaks.
    pipeline.upsert_scene("__safe__", library.require("__safe__")?)?;
    // Compile the baked __video__ scene up front so GLSL errors surface at
    // boot, not at the first slot bind. Parity with __safe__.
    pipeline.upsert_scene("__video__", library.require("__video__")?)?;
    // Start the video capture worker and hand the read-side to the pipeline.
    // start_capture is non-blocking; the worker reports status via ArcSwap.
    let video_prefs = mandlerot::video::VideoPrefs {
        device: None,
        target_width: 720,
        target_height: 480,
    };
    let video = mandlerot::video::start_capture(video_prefs);
    pipeline.attach_video_handle(Some(video));
    pipeline.upsert_scene(
        &state.layer_a.scene_name,
        library.require(&state.layer_a.scene_name)?,
    )?;
    pipeline.upsert_scene(
        &state.layer_b.scene_name,
        library.require(&state.layer_b.scene_name)?,
    )?;
    // Best-effort: load post-FX passes. Failures inside `load_dir` are
    // already logged + skipped per-pass; a missing directory just yields an
    // empty chain (no-op when `frame()` checks `has_enabled`).
    if let Err(e) = pipeline.postfx_load_dir(&cli.postfx) {
        tracing::warn!("postfx load_dir({:?}) failed: {e}; running without post-FX", cli.postfx);
    } else {
        // Layer the user's saved enable/param overrides over the defaults.
        if let Err(e) = pipeline.postfx.load_state(&state_dir) {
            tracing::warn!("postfx load_state: {e}; using built-in defaults");
        }
        let enabled = pipeline
            .postfx
            .passes()
            .iter()
            .filter(|p| p.enabled)
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>();
        tracing::info!(
            "postfx: {} pass(es) loaded, enabled={:?}",
            pipeline.postfx.passes().len(),
            enabled
        );
    }

    let watcher = HotReloader::watch(&cli.scenes).context("hot watcher")?;
    let postfx_watcher = match HotReloader::watch_postfx(&cli.postfx) {
        Ok(w) => Some(w),
        Err(e) => {
            tracing::warn!("postfx hot-reload disabled: {e}");
            None
        }
    };

    let audio_atomic = Arc::new(AtomicAudio::new());
    let mut audio_stop = Arc::new(AtomicBool::new(false));
    let audio_history = mandlerot::audio::history::AudioHistory::new();
    // Pre-allocated scratch buffer that `AudioHistory::snapshot_into`
    // refills every frame. Sized to the fixed 1×320 RGBA8 texture
    // dimensions; never grows.
    let mut audio_history_scratch =
        vec![0u8; mandlerot::audio::history::HISTORY_LEN * 4];
    // Track the device name the audio thread was started with so we can detect
    // changes from the AudioDeviceScreen picker and respawn the worker.
    let mut current_audio_device: String = audio_params.device();
    let mut audio_thread: Option<JoinHandle<()>> = if cli.replay.is_none() {
        Some(spawn_audio(
            audio_atomic.clone(),
            audio_params.clone(),
            audio_stop.clone(),
            Some(current_audio_device.clone()),
        ))
    } else {
        // Replay mode: no live capture, so the audio thread doesn't run; treat
        // audio as absent so the default bypass kicks in below.
        audio_atomic.set_present(false);
        audio_atomic.mark_probed();
        None
    };
    // Default `audio_bypass=true` if no input device was detected. The audio
    // thread probes the device for up to 1s; wait up to ~1.2s for the probe
    // to flip `probed=true`, then sample `present`. Live capture sets
    // `probed` as soon as the first sample arrives, so this is fast in the
    // common case.
    {
        let probe_deadline = Instant::now() + Duration::from_millis(1200);
        while !audio_atomic.is_probed() && Instant::now() < probe_deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        if !audio_atomic.is_present() {
            state.audio_bypass = true;
            tracing::info!("no audio input detected; defaulting audio_bypass=ON");
        }
    }

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
    let (status_handle, _status_join) = mandlerot::status::thread::spawn(status_backend);

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
    let mut fps_window_start = Instant::now();
    let mut fps_window_frames = 0u32;
    let mut fps_window_render_us = 0u64;
    // Cached visible-scene names. Rebuilt only when hot-reload fires —
    // before this, every UI key event AND every render-with-menu rebuilt
    // it from scratch via `library.names().map(...).collect()`, which is a
    // 58-string allocation pair every time.
    let mut scene_names: Vec<String> = library
        .names()
        .filter(|n| !n.starts_with("__"))
        .map(|s| s.to_string())
        .collect();

    if let Some(frames_per_scene) = cli.benchmark {
        let frames = frames_per_scene.max(30) as usize;
        state.audio_bypass = true;
        let scan = target.dimensions();
        let names = scene_names.clone();
        // Reference scene for the xfade=0.5 phase. Plasma is a cheap full-screen
        // gradient so the measurement reflects layer-A cost + blend, not layer B.
        const XFADE_REF: &str = "plasma";
        let xfade_ref_compiled = match library.require(XFADE_REF) {
            Ok(s) => pipeline.upsert_scene(XFADE_REF, s).map(|_| true).unwrap_or(false),
            Err(_) => false,
        };
        let postfx_names: Vec<String> =
            pipeline.postfx.passes().iter().map(|p| p.name.clone()).collect();
        // Start fully disabled.
        for p in pipeline.postfx.passes_mut().iter_mut() {
            p.enabled = false;
        }
        tracing::info!(
            "benchmark: {} scenes, {} frames/run, postfx passes={}, xfade_ref={}",
            names.len(),
            frames,
            postfx_names.len(),
            if xfade_ref_compiled { XFADE_REF } else { "<missing>" }
        );

        // Closure-free measurement helper, inlined per phase to avoid borrowing
        // pipeline mutably while also reading state.
        for name in &names {
            let Ok(loaded) = library.require(name) else {
                tracing::warn!("bench: skip {} (library require failed)", name);
                continue;
            };
            if let Err(e) = pipeline.upsert_scene(name, loaded) {
                tracing::warn!("bench: skip {} (compile failed: {})", name, e);
                continue;
            }
            state.layer_a.scene_name = name.clone();
            state.layer_a.params = mandlerot::scene::ParamMap::from_scene(&loaded.meta);
            state.layer_b.scene_name = if xfade_ref_compiled {
                XFADE_REF.to_string()
            } else {
                name.clone()
            };
            // Reset layer-B params to its meta defaults too.
            if let Ok(b_meta) = library.require(&state.layer_b.scene_name) {
                state.layer_b.params =
                    mandlerot::scene::ParamMap::from_scene(&b_meta.meta);
            }

            // === Phase: base (xfade=0, no postfx) ===
            state.xfade = 0.0;
            for p in pipeline.postfx.passes_mut().iter_mut() {
                p.enabled = false;
            }
            measure(
                &mut pipeline,
                &mut target,
                &state,
                scan,
                frames,
                |fps, ms| {
                    tracing::info!(
                        "bench: phase=base scene={} fps={:.2} avg_render_ms={:.2}",
                        name, fps, ms
                    );
                },
            )?;

            // === Phase: xfade50 (light reference scene on B) ===
            if xfade_ref_compiled {
                state.xfade = 0.5;
                measure(
                    &mut pipeline,
                    &mut target,
                    &state,
                    scan,
                    frames,
                    |fps, ms| {
                        tracing::info!(
                            "bench: phase=xfade50 scene_a={} scene_b={} fps={:.2} avg_render_ms={:.2}",
                            name, XFADE_REF, fps, ms
                        );
                    },
                )?;
                state.xfade = 0.0;
            }

            // === Phase: xfade50_heavy (scene_b = scene_a, worst-case blend) ===
            state.layer_b.scene_name = name.clone();
            state.layer_b.params =
                mandlerot::scene::ParamMap::from_scene(&loaded.meta);
            state.xfade = 0.5;
            measure(
                &mut pipeline,
                &mut target,
                &state,
                scan,
                frames,
                |fps, ms| {
                    tracing::info!(
                        "bench: phase=xfade50_heavy scene_a={} scene_b={} fps={:.2} avg_render_ms={:.2}",
                        name, name, fps, ms
                    );
                },
            )?;
            state.xfade = 0.0;
            // Restore layer-B to the reference scene for subsequent phases.
            if xfade_ref_compiled {
                state.layer_b.scene_name = XFADE_REF.to_string();
                if let Ok(b_meta) = library.require(XFADE_REF) {
                    state.layer_b.params =
                        mandlerot::scene::ParamMap::from_scene(&b_meta.meta);
                }
            }

            // === Phase: postfx (one pass at a time, xfade=0) ===
            for fx in &postfx_names {
                for p in pipeline.postfx.passes_mut().iter_mut() {
                    p.enabled = p.name == *fx;
                }
                let fx_name = fx.clone();
                measure(
                    &mut pipeline,
                    &mut target,
                    &state,
                    scan,
                    frames,
                    |fps, ms| {
                        tracing::info!(
                            "bench: phase=postfx scene={} fx={} fps={:.2} avg_render_ms={:.2}",
                            name, fx_name, fps, ms
                        );
                    },
                )?;
            }
            // Disable all after the scene's postfx sweep so the next scene's
            // base phase starts clean.
            for p in pipeline.postfx.passes_mut().iter_mut() {
                p.enabled = false;
            }
        }
        tracing::info!("benchmark: done");
        return Ok(());
    }

    // Demo: cycle scenes at fixed dwell. Lives alongside the normal render
    // loop so the status panel, audio path, and 30fps pacing stay identical
    // to live use — only the scene rotation is automated.
    let mut demo: Option<(Vec<String>, usize, Instant, Duration)> =
        cli.demo_seconds.map(|secs| {
            // Force postfx off, xfade=0 — we're verifying scene visual
            // fidelity, not the post chain or blend.
            for p in pipeline.postfx.passes_mut().iter_mut() {
                p.enabled = false;
            }
            state.xfade = 0.0;
            let dwell = Duration::from_secs(secs.max(1) as u64);
            let names = scene_names.clone();
            if let Some(first) = names.first() {
                if let Ok(loaded) = library.require(first) {
                    let _ = pipeline.upsert_scene(first, loaded);
                    state.layer_a.scene_name = first.clone();
                    state.layer_a.params =
                        mandlerot::scene::ParamMap::from_scene(&loaded.meta);
                }
                tracing::info!("demo: scene={} ({}s)", first, secs);
            }
            (names, 0, Instant::now(), dwell)
        });

    // Smoothed render fps for the status panel readout. EMA over wall-clock
    // delta between successive frame starts so it reflects what the panel
    // actually displays (i.e. capped at the 30fps pacing budget when GPU has
    // headroom, dropping under load).
    let mut last_frame_start: Option<Instant> = None;
    let mut fps_smoothed: f32 = 0.0;
    let mut fps_have: bool = false;

    loop {
        let frame_start = Instant::now();
        if let Some(prev) = last_frame_start {
            let dt = frame_start.duration_since(prev).as_secs_f32();
            if dt > 1e-6 {
                let instant = 1.0 / dt;
                if fps_have {
                    fps_smoothed = fps_smoothed * 0.85 + instant * 0.15;
                } else {
                    fps_smoothed = instant;
                    fps_have = true;
                }
            }
        }
        last_frame_start = Some(frame_start);

        #[cfg(target_os = "linux")]
        if let Some(w) = watchdog.as_mut() {
            w.pet();
        }

        // Snapshot capture status once per frame — cheap (ArcSwap load) and
        // gives a stable value to thread through the input + render contexts
        // and the panel snapshot below.
        let video_status = pipeline.video_status();

        // Hot-reload
        let mut scenes_dirty = false;
        while let Some(evt) = watcher.try_recv() {
            handle_reload(evt, &cli.scenes, &mut library, &mut pipeline);
            scenes_dirty = true;
        }
        if scenes_dirty {
            scene_names = library
                .names()
                .filter(|n| !n.starts_with("__"))
                .map(|s| s.to_string())
                .collect();
        }

        // Hot-reload: postfx dir
        if let Some(pw) = postfx_watcher.as_ref() {
            let mut postfx_dirty = false;
            while let Some(evt) = pw.try_recv() {
                match evt {
                    ReloadEvent::PostFxTouched { .. } | ReloadEvent::PostFxRemoved { .. } => {
                        postfx_dirty = true;
                    }
                    _ => {}
                }
            }
            if postfx_dirty {
                match pipeline.postfx_load_dir(&cli.postfx) {
                    Ok(()) => tracing::info!("postfx: hot-reload applied"),
                    Err(e) => tracing::warn!("postfx: hot-reload failed: {e}"),
                }
                if let Err(e) = looks
                    .after_postfx_mutation(state.active_look_slot, pipeline.postfx.snapshot())
                {
                    tracing::warn!("postfx auto-sync on hot-reload: {e}");
                }
            }
        }

        // Audio sample
        let bands = audio_atomic.load_bands();
        let beat_value = audio_atomic.load_beat();
        state.audio_bands = bands;
        // Record this frame's bands into the rolling history ring and push
        // the resulting RGBA snapshot to the GPU. The history texture is
        // RGBA8 (4 channels), so only the legacy four bands (bass, lomid,
        // himid, treble) participate in `u_audio_history`. The Mid band
        // is reachable as a live value via `u_audio_mid` but has no
        // historical waveform — scenes that want a Mid spectrogram would
        // need a second history texture.
        audio_history.push([bands[0], bands[1], bands[2], bands[3]]);
        audio_history.snapshot_into(&mut audio_history_scratch);
        pipeline.upload_audio_history(&audio_history_scratch);
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

        // Demo: advance scene when dwell elapses; exit after the last.
        if let Some((names, idx, scene_start, dwell)) = demo.as_mut() {
            if scene_start.elapsed() >= *dwell {
                *idx += 1;
                if *idx >= names.len() {
                    tracing::info!("demo: done");
                    break;
                }
                let next = names[*idx].clone();
                if let Ok(loaded) = library.require(&next) {
                    let _ = pipeline.upsert_scene(&next, loaded);
                    state.layer_a.scene_name = next.clone();
                    state.layer_a.params =
                        mandlerot::scene::ParamMap::from_scene(&loaded.meta);
                }
                *scene_start = Instant::now();
                tracing::info!("demo: scene={} ({:?})", next, dwell);
            }
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
            // Double-tap Esc fires Panic *and* closes any open menu — runs
            // before stack dispatch so menus can't swallow it.
            if key.as_str() == "Esc" && esc_double.tap(Instant::now()) {
                ui_stack.close_all();
                handle_action(
                    &Action::Panic,
                    &mut state,
                    &library,
                    &mut looks,
                    &mut tap_tempo,
                    &mut pipeline.postfx,
                );
                continue;
            }
            // Numpad Panic chord: `-` + `+` + `Enter` within 400 ms (any
            // order, any order of events). Same global semantics as the
            // double-esc — the chord fires even when a menu is open.
            if panic_chord.observe(&key, Instant::now()) {
                ui_stack.close_all();
                handle_action(
                    &Action::Panic,
                    &mut state,
                    &library,
                    &mut looks,
                    &mut tap_tempo,
                    &mut pipeline.postfx,
                );
                continue;
            }
            // While a menu screen is open, all key events go to it. The menu
            // operates on bindings (persistent) and never touches the live
            // shader state, so the running visuals stay in sync regardless.
            if ui_stack.is_open() {
                let mut ctx = ScreenCtx {
                    scenes: &scene_names,
                    bindings: &mut state.slot_bindings,
                    state_dir: &state_dir,
                    audio: &audio_params,
                    postfx: Some(&mut pipeline.postfx),
                    chromakey: Some(&mut state.chromakey),
                    video_status,
                    active_look_slot: state.active_look_slot,
                    looks: Some(&mut looks),
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
                        MenuKind::Looks => {} // placeholder until Task 6
                    }
                    continue;
                }
                let old_a = state.layer_a.scene_name.clone();
                let old_b = state.layer_b.scene_name.clone();
                handle_action(
                    &action,
                    &mut state,
                    &library,
                    &mut looks,
                    &mut tap_tempo,
                    &mut pipeline.postfx,
                );
                if matches!(action, mandlerot::action::Action::ChromakeyToggle) {
                    if let Err(e) = state.chromakey.save(&state_dir) {
                        tracing::warn!("save chromakey.toml: {e}");
                    }
                }
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
        // Substitute disabled scenes with __safe__. Compare via borrow so
        // the steady-state path allocates nothing — `resolve` returns a
        // `&str` into the supervisor table.
        {
            let resolved = supervisor.resolve(&state.layer_a.scene_name);
            if resolved != state.layer_a.scene_name {
                state.layer_a.scene_name = resolved.to_string();
            }
        }
        {
            let resolved = supervisor.resolve(&state.layer_b.scene_name);
            if resolved != state.layer_b.scene_name {
                state.layer_b.scene_name = resolved.to_string();
            }
        }

        // If a menu is open, paint it on the main thread (where bindings and
        // scene names live) and send the rendered grid; the status worker
        // blits it verbatim instead of composing from state.
        let menu_grid = if ui_stack.is_open() {
            let bound_state = state.active_look_slot.map(|s| {
                (s, looks.has_snapshot(s), looks.is_bound_active(s))
            });
            let rctx = RenderCtx {
                scenes: &scene_names,
                bindings: &state.slot_bindings,
                audio: &audio_params,
                postfx: Some(&pipeline.postfx),
                chromakey: Some(&state.chromakey),
                filtered_scenes: filtered,
                pi_gen,
                video_status,
                active_look_slot: state.active_look_slot,
                bound_state,
            };
            ui_stack.render_top(&rctx)
        } else {
            None
        };
        let mut panel = mandlerot::status::snapshot::PanelSnapshot::from_state(&state);
        panel.video_status = video_status;
        panel.chromakey_chip = mandlerot::status::snapshot::ChromakeyChip::from_state(&state.chromakey);
        panel.look_postfx_bound = state
            .active_look_slot
            .map(|s| looks.is_bound_active(s))
            .unwrap_or(false);
        status_handle.try_send(mandlerot::status::thread::StateSnapshot {
            panel,
            menu_grid,
            postfx_summary: pipeline.postfx.summary_tag().to_string(),
            fps: if fps_have { Some(fps_smoothed) } else { None },
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

        // Once per second (~60 frames), check whether the operator selected a
        // different capture device via F4 → Audio → Device. On change: stop
        // the old worker, join it, then spawn a fresh worker with the new
        // device. cpal stream-drop takes a few ms which briefly blocks the
        // render loop — acceptable for an operator-initiated switch.
        if cli.replay.is_none() && frame % 60 == 0 {
            let now_dev = audio_params.device();
            if now_dev != current_audio_device {
                tracing::info!(
                    "audio: device change '{}' -> '{}', respawning worker",
                    current_audio_device,
                    now_dev
                );
                audio_stop.store(true, Ordering::Relaxed);
                if let Some(h) = audio_thread.take() {
                    let _ = h.join();
                }
                audio_stop = Arc::new(AtomicBool::new(false));
                audio_thread = Some(spawn_audio(
                    audio_atomic.clone(),
                    audio_params.clone(),
                    audio_stop.clone(),
                    Some(now_dev.clone()),
                ));
                current_audio_device = now_dev;
            }
        }

        let elapsed = frame_start.elapsed();
        fps_window_frames += 1;
        fps_window_render_us += elapsed.as_micros() as u64;
        if fps_window_start.elapsed() >= Duration::from_secs(5) {
            let wall_s = fps_window_start.elapsed().as_secs_f64();
            let fps = fps_window_frames as f64 / wall_s;
            let avg_render_ms = (fps_window_render_us as f64 / fps_window_frames as f64) / 1000.0;
            let budget_ms = frame_dt.as_micros() as f64 / 1000.0;
            tracing::info!(
                "fps: {:.1} avg_render={:.2}ms budget={:.1}ms scene_a={} scene_b={}",
                fps,
                avg_render_ms,
                budget_ms,
                state.layer_a.scene_name,
                state.layer_b.scene_name,
            );
            fps_window_start = Instant::now();
            fps_window_frames = 0;
            fps_window_render_us = 0;
        }
        if elapsed < frame_dt {
            std::thread::sleep(frame_dt - elapsed);
        }
    }

    audio_stop.store(true, Ordering::Relaxed);
    if let Some(h) = audio_thread.take() {
        let _ = h.join();
    }
    status_handle.stop.store(true, Ordering::Relaxed);
    Ok(())
}

fn handle_action(
    action: &Action,
    state: &mut SharedState,
    lib: &SceneLibrary,
    looks: &mut LookStore,
    tap: &mut TapTempo,
    postfx: &mut PostFx,
) {
    // (The status panel's "LAST" cell was retired when the AUDIO/LOOKS
    // section went 50/50; we used to populate `state.last_action_label`
    // here so the panel could show it. Keeping the field for now to avoid
    // an API churn, but no producer writes to it so the per-frame state
    // clone stays allocation-free for that String.)
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
            } else if let Err(e) =
                looks.recall(slot, state, lib, |snap| postfx.apply_snapshot(snap))
            {
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
        ReloadEvent::PostFxTouched { .. } | ReloadEvent::PostFxRemoved { .. } => return,
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
