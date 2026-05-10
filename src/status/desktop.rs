//! Desktop status backends:
//! - `DesktopPngBackend` — writes the framebuffer as PNG to a path on demand.
//!   Useful for verifying the panel design from macOS without an actual SPI panel.
//! - `DesktopBufferBackend` — shares the framebuffer via `Arc<Mutex<Vec<u16>>>` so
//!   the main thread can display it in a live preview window (requires `--status-window`).

use std::path::Path;
use std::sync::{Arc, Mutex};

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::RgbColor;

use super::render::Fb;

pub struct DesktopPngBackend {
    pub out_path: std::path::PathBuf,
}

impl DesktopPngBackend {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            out_path: path.as_ref().to_path_buf(),
        }
    }
}

impl super::Backend for DesktopPngBackend {
    fn flush_full(&mut self, fb: &Fb) -> crate::Result<()> {
        let mut rgba = Vec::with_capacity((fb.width * fb.height * 4) as usize);
        for px in &fb.data {
            let (r, g, b) = rgb565_to_rgb888(*px);
            rgba.push(r);
            rgba.push(g);
            rgba.push(b);
            rgba.push(255);
        }
        if let Some(parent) = self.out_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        image::save_buffer(
            &self.out_path,
            &rgba,
            fb.width,
            fb.height,
            image::ColorType::Rgba8,
        )
        .map_err(|e| crate::Error::Backend(format!("png save: {e}")))
    }

    fn flush_runs(&mut self, fb: &Fb, _runs: &[(usize, usize, usize)]) -> crate::Result<()> {
        // Cheap: just rewrite the whole PNG. The dev sink isn't perf-critical.
        self.flush_full(fb)
    }
}

fn rgb565_to_rgb888(p: Rgb565) -> (u8, u8, u8) {
    let r = (p.r() as u16 * 255 / 31) as u8;
    let g = (p.g() as u16 * 255 / 63) as u8;
    let b = (p.b() as u16 * 255 / 31) as u8;
    (r, g, b)
}

/// A backend that exposes the framebuffer as a shared `Arc<Mutex<Vec<u16>>>` (RGB565,
/// 480×320 = 153 600 entries) so the main thread can paint it into a live window.
///
/// Used when `--status-window` is passed; the `buf` clone is handed to
/// `WinitGlTarget::enable_status_window`.
#[derive(Clone)]
pub struct DesktopBufferBackend {
    /// RGB565 packed pixels, row-major, 480 wide × 320 tall.
    pub buf: Arc<Mutex<Vec<u16>>>,
}

impl Default for DesktopBufferBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopBufferBackend {
    pub fn new() -> Self {
        Self {
            buf: Arc::new(Mutex::new(vec![0u16; 480 * 320])),
        }
    }
}

impl super::Backend for DesktopBufferBackend {
    fn flush_full(&mut self, fb: &Fb) -> crate::Result<()> {
        let mut buf = self.buf.lock().unwrap();
        for (i, px) in fb.data.iter().enumerate() {
            let r = px.r() as u16;
            let g = px.g() as u16;
            let b = px.b() as u16;
            buf[i] = (r << 11) | (g << 5) | b;
        }
        Ok(())
    }

    fn flush_runs(&mut self, fb: &Fb, _runs: &[(usize, usize, usize)]) -> crate::Result<()> {
        // Full refresh is cheap enough at 10 Hz (300 KB) and simpler than
        // tracking dirty regions in the shared buffer.
        self.flush_full(fb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::theme::FG_BRIGHT;
    use crate::status::Backend;

    #[test]
    fn rgb565_white_to_rgb888_is_full_brightness() {
        let (r, g, b) = rgb565_to_rgb888(Rgb565::new(31, 63, 31));
        assert_eq!((r, g, b), (255, 255, 255));
    }

    #[test]
    fn writes_a_png() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.png");
        let mut be = DesktopPngBackend::new(&path);
        let mut fb = Fb::new(8, 8);
        fb.fill_rect(0, 0, 8, 8, FG_BRIGHT);
        be.flush_full(&fb).unwrap();
        assert!(path.exists());
        let meta = std::fs::metadata(&path).unwrap();
        assert!(meta.len() > 0);
    }
}
