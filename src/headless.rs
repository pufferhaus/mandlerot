//! Headless render runner — opens a hidden window for context, draws N
//! frames, optionally writes PNGs. Used for smoke tests and CI.

use std::path::Path;
use std::sync::Arc;

use crate::error::Result;
use crate::render::desktop::WinitGlTarget;
use crate::render::pipeline::Pipeline;
use crate::render::target::RenderTarget;
use crate::scene::SceneLibrary;
use crate::state::{BlendMode, SharedState};

pub struct HeadlessRun {
    pub frames: u32,
    pub scene_a: String,
    pub scene_b: String,
    pub xfade: f32,
    pub blend_mode: BlendMode,
    pub width: u32,
    pub height: u32,
    pub dump_to: Option<std::path::PathBuf>,
}

impl HeadlessRun {
    pub fn run(self, lib: &SceneLibrary) -> Result<Vec<Vec<u8>>> {
        let mut target = WinitGlTarget::new(self.width, self.height, "mandlerot-headless")?;
        let gl: Arc<glow::Context> = target.gl();
        let mut pipeline = Pipeline::new(gl, self.width, self.height)?;
        pipeline.upsert_scene(&self.scene_a, lib.require(&self.scene_a)?)?;
        pipeline.upsert_scene(&self.scene_b, lib.require(&self.scene_b)?)?;

        let mut state = SharedState::from_initial(
            lib,
            &self.scene_a,
            &self.scene_b,
            self.xfade,
            self.blend_mode,
        )?;

        let mut captures = Vec::new();
        for frame in 0..self.frames {
            state.time_secs = frame as f32 / 30.0;
            pipeline.frame(&state, self.width, self.height)?;
            let pixels = pipeline.read_default_pixels(self.width, self.height);
            if let Some(dir) = &self.dump_to {
                std::fs::create_dir_all(dir)?;
                let path = dir.join(format!("frame_{frame:04}.png"));
                save_rgba_png(&path, self.width, self.height, &pixels)?;
            }
            captures.push(pixels);
            target.present()?;
            if !target.pump() {
                break;
            }
        }
        Ok(captures)
    }
}

fn save_rgba_png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Result<()> {
    // glReadPixels returns the image flipped vertically vs PNG convention.
    let row = (w * 4) as usize;
    let mut flipped = vec![0u8; rgba.len()];
    for y in 0..h as usize {
        let src = &rgba[y * row..(y + 1) * row];
        let dst = &mut flipped[(h as usize - 1 - y) * row..(h as usize - y) * row];
        dst.copy_from_slice(src);
    }
    image::save_buffer(path, &flipped, w, h, image::ColorType::Rgba8)
        .map_err(|e| crate::Error::Backend(format!("png save: {e}")))?;
    Ok(())
}
