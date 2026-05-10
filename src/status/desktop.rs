//! Desktop status backend — writes the framebuffer as PNG to a path on demand.
//! Useful for verifying the panel design from macOS without an actual SPI panel.

#![cfg(feature = "desktop")]

use std::path::Path;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::Backend;
    use crate::status::theme::FG_BRIGHT;

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
