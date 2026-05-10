use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::Parser;

use mandlerot::config::Config;
use mandlerot::hot_reload::{HotReloader, ReloadEvent};
#[cfg(all(feature = "desktop", not(feature = "pi")))]
use mandlerot::render::desktop::WinitGlTarget;
use mandlerot::render::pipeline::Pipeline;
use mandlerot::render::target::RenderTarget;
use mandlerot::scene::{LoadedScene, SceneLibrary, SceneMeta};
use mandlerot::state::{BlendMode, SharedState};

#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Path to config.toml
    #[arg(long, default_value = "config.toml")]
    config: PathBuf,

    /// Path to scenes directory
    #[arg(long, default_value = "scenes")]
    scenes: PathBuf,

    /// Headless smoke test: render N frames and exit
    #[arg(long)]
    smoke_frames: Option<u32>,
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
    let library = SceneLibrary::load_dir(&cli.scenes).context("load scenes")?;
    let blend_mode = BlendMode::parse(&cfg.initial.blend_mode).unwrap_or(BlendMode::Mix);
    let mut state = SharedState::from_initial(
        &library,
        &cfg.initial.scene_a,
        &cfg.initial.scene_b,
        cfg.initial.xfade,
        blend_mode,
    )?;

    #[cfg(all(feature = "desktop", not(feature = "pi")))]
    let mut target: Box<dyn RenderTarget> = Box::new(WinitGlTarget::new(
        cfg.render.width,
        cfg.render.height,
        "mandleROT",
    )?);
    #[cfg(all(feature = "pi", target_os = "linux"))]
    let mut target: Box<dyn RenderTarget> = {
        use mandlerot::render::pi::PiTarget;
        Box::new(PiTarget::new(cfg.render.width, cfg.render.height)?)
    };
    let gl: Arc<glow::Context> = target.gl();
    let mut pipeline = Pipeline::new(gl, cfg.render.width, cfg.render.height)?;
    pipeline.upsert_scene(
        &state.layer_a.scene_name,
        library.require(&state.layer_a.scene_name)?,
    )?;
    pipeline.upsert_scene(
        &state.layer_b.scene_name,
        library.require(&state.layer_b.scene_name)?,
    )?;

    let mut library = library; // make mutable for hot-reload
    let watcher = HotReloader::watch(&cli.scenes).context("hot watcher")?;
    let start = Instant::now();
    let frame_dt = Duration::from_micros(1_000_000 / cfg.render.fps as u64);
    let mut frame = 0u64;

    loop {
        let frame_start = Instant::now();

        // Drain any reload events
        while let Some(evt) = watcher.try_recv() {
            handle_reload(evt, &cli.scenes, &mut library, &mut pipeline);
        }

        state.time_secs = start.elapsed().as_secs_f32();
        if let Err(e) = pipeline.frame(&state, cfg.render.width, cfg.render.height) {
            tracing::error!("frame error: {e}");
        }
        target.present()?;
        if !target.pump() {
            tracing::info!("exit requested");
            break;
        }
        if let Some(n) = cli.smoke_frames {
            if frame as u32 >= n {
                tracing::info!("smoke frames complete");
                break;
            }
        }
        frame += 1;

        // Coarse frame pacing
        let elapsed = frame_start.elapsed();
        if elapsed < frame_dt {
            std::thread::sleep(frame_dt - elapsed);
        }
    }
    Ok(())
}

fn handle_reload(
    evt: ReloadEvent,
    scenes_dir: &Path,
    library: &mut SceneLibrary,
    pipeline: &mut Pipeline,
) {
    let stem = match &evt {
        ReloadEvent::SceneTouched { stem } | ReloadEvent::SceneRemoved { stem } => stem.clone(),
    };
    if let ReloadEvent::SceneRemoved { .. } = evt {
        tracing::info!("scene removed: {stem}");
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
    let scene = LoadedScene {
        meta: meta.clone(),
        fragment_body: body,
        source_path: glsl_path.clone(),
    };
    library.upsert(&stem, scene.clone());
    if pipeline.has_scene(&stem) || pipeline.upsert_scene(&stem, &scene).is_ok() {
        // Already-known scene → recompile in place
        if let Err(e) = pipeline.upsert_scene(&stem, &scene) {
            tracing::warn!("recompile {stem}: {e}");
        } else {
            tracing::info!("hot-reloaded {stem}");
        }
    }
}
