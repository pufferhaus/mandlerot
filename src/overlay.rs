//! Production overlay: a single-line text strip at the top of the composite
//! output, toggleable via `state.status_overlay_visible`. The text is
//! pre-rasterized into a small RGBA texture each frame using the same
//! `FONT_6X12` font as the status panel.

use embedded_graphics::mono_font::ascii::FONT_6X12;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text, TextStyleBuilder};

use crate::state::{Layer, Mode, SharedState};

pub const STRIP_W: u32 = 360;
pub const STRIP_H: u32 = 12;

pub fn build_strip_text(state: &SharedState) -> String {
    let mode = match state.active_mode {
        Mode::Scene => "SCN",
        Mode::Param => "PRM",
        Mode::Look => "LK",
    };
    let layer = match state.active_layer {
        Layer::A => 'A',
        Layer::B => 'B',
    };
    let blend = match state.blend_mode {
        crate::state::BlendMode::Mix => "mix",
        crate::state::BlendMode::Add => "add",
        crate::state::BlendMode::Multiply => "mult",
        crate::state::BlendMode::Screen => "screen",
        crate::state::BlendMode::Difference => "diff",
        crate::state::BlendMode::Overlay => "overly",
        crate::state::BlendMode::HardLight => "hardlt",
        crate::state::BlendMode::Lighten => "lgt",
        crate::state::BlendMode::Darken => "drk",
        crate::state::BlendMode::Exclusion => "excl",
        crate::state::BlendMode::Subtract => "sub",
        crate::state::BlendMode::LinearBurn => "linbn",
        crate::state::BlendMode::SoftLight => "softlt",
        crate::state::BlendMode::ColorDodge => "coldg",
        crate::state::BlendMode::ColorBurn => "colbn",
        crate::state::BlendMode::Hue => "hue",
        crate::state::BlendMode::Saturation => "sat",
        crate::state::BlendMode::Color => "color",
        crate::state::BlendMode::Luminosity => "lumin",
    };
    format!(
        "{} L:{} A:{} B:{} X={:.2} BL:{}",
        mode,
        layer,
        truncate(&state.layer_a.scene_name, 8),
        truncate(&state.layer_b.scene_name, 8),
        state.xfade,
        blend,
    )
}

fn truncate(s: &str, max: usize) -> String {
    // Char-aware: byte-slicing on a UTF-8 boundary mid-codepoint panics, and
    // scene names / labels can contain non-ASCII glyphs that get substituted
    // later. Counting chars caps "visual width" close enough for status text.
    s.chars().take(max).collect()
}

/// Rasterize the overlay text into RGBA8 bytes (`STRIP_W × STRIP_H × 4`).
pub fn rasterize(text: &str) -> Vec<u8> {
    let mut buf = vec![0u8; (STRIP_W * STRIP_H * 4) as usize];
    let amber = Rgb888::new(0xFF, 0xB0, 0x00);
    let mut canvas = RgbaBuf {
        w: STRIP_W,
        h: STRIP_H,
        data: &mut buf,
    };
    let style = MonoTextStyle::new(&FONT_6X12, amber);
    let text_style = TextStyleBuilder::new().baseline(Baseline::Top).build();
    let _ = Text::with_text_style(text, Point::new(0, 0), style, text_style).draw(&mut canvas);
    buf
}

struct RgbaBuf<'a> {
    w: u32,
    h: u32,
    data: &'a mut [u8],
}

impl OriginDimensions for RgbaBuf<'_> {
    fn size(&self) -> Size {
        Size::new(self.w, self.h)
    }
}

impl DrawTarget for RgbaBuf<'_> {
    type Color = Rgb888;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(p, c) in pixels {
            if p.x >= 0 && (p.x as u32) < self.w && p.y >= 0 && (p.y as u32) < self.h {
                let idx = ((p.y as u32 * self.w + p.x as u32) * 4) as usize;
                self.data[idx] = c.r();
                self.data[idx + 1] = c.g();
                self.data[idx + 2] = c.b();
                self.data[idx + 3] = 255;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{LoadedScene, SceneLibrary, SceneMeta};
    use crate::state::{BlendMode, SharedState};

    fn lib() -> SceneLibrary {
        let mut l = SceneLibrary::default();
        for n in ["plasma", "solid"] {
            let meta = SceneMeta::parse(&format!("name = \"{n}\"\n"), "x").unwrap();
            l.upsert(
                n,
                LoadedScene {
                    meta,
                    fragment_body: "void main() {}".into(),
                    source_path: std::path::PathBuf::from("inline"),
                    is_hq: false,
                },
            );
        }
        l
    }

    #[test]
    fn strip_text_includes_mode() {
        let s = SharedState::from_initial(&lib(), "plasma", "solid", 0.5, BlendMode::Mix).unwrap();
        let t = build_strip_text(&s);
        // Default-mode start is Param so the strip label reads "PRM".
        assert!(t.contains("PRM"));
        assert!(t.contains("L:A"));
        assert!(t.contains("X=0.50"));
    }

    #[test]
    fn rasterize_returns_correct_size() {
        let buf = rasterize("test");
        assert_eq!(buf.len(), (STRIP_W * STRIP_H * 4) as usize);
    }

    #[test]
    fn rasterize_paints_some_pixels() {
        let buf = rasterize("X");
        let any_lit = buf.chunks_exact(4).any(|p| p[0] > 0 || p[1] > 0);
        assert!(any_lit);
    }
}
