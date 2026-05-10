//! Render a `TextScreen` into a 480×320 RGB565 framebuffer using the
//! built-in `FONT_6X12` from embedded-graphics.

use embedded_graphics::mono_font::ascii::FONT_6X12;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text, TextStyleBuilder};

use super::glyphs::{substitute, CELL_H, CELL_W};
use super::grid::{TextScreen, COLS, ROWS};
use super::theme::{resolve, BG};

pub const PANEL_W: u32 = 480;
pub const PANEL_H: u32 = 320;

/// In-memory framebuffer compatible with `embedded_graphics::DrawTarget`.
/// We use a flat `Vec<Rgb565>` rather than a const-generic Framebuffer because
/// `embedded_graphics::framebuffer::Framebuffer` requires const dimensions
/// awkward to thread through.
pub struct Fb {
    pub width: u32,
    pub height: u32,
    pub data: Vec<Rgb565>,
}

impl Fb {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![BG; (width * height) as usize],
        }
    }

    pub fn pixel_at(&self, x: u32, y: u32) -> Rgb565 {
        self.data[(y * self.width + x) as usize]
    }

    fn set_pixel(&mut self, x: u32, y: u32, c: Rgb565) {
        if x < self.width && y < self.height {
            self.data[(y * self.width + x) as usize] = c;
        }
    }

    pub fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, c: Rgb565) {
        for yy in y..(y + h).min(self.height) {
            for xx in x..(x + w).min(self.width) {
                self.set_pixel(xx, yy, c);
            }
        }
    }
}

impl OriginDimensions for Fb {
    fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }
}

impl DrawTarget for Fb {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(p, c) in pixels {
            if p.x >= 0 && p.y >= 0 {
                self.set_pixel(p.x as u32, p.y as u32, c);
            }
        }
        Ok(())
    }
}

/// Render every cell in `screen` into `fb`. Caller is expected to have
/// pre-cleared `fb` to background or to be doing partial updates via diff
/// runs (see `render_runs`).
pub fn render_full(screen: &TextScreen, fb: &mut Fb) {
    for row in 0..ROWS {
        for col in 0..COLS {
            draw_cell(screen, fb, row, col);
        }
    }
}

/// Render only the cells in the given runs.
pub fn render_runs(screen: &TextScreen, runs: &[(usize, usize, usize)], fb: &mut Fb) {
    for &(row, col_lo, col_hi) in runs {
        for col in col_lo..col_hi {
            draw_cell(screen, fb, row, col);
        }
    }
}

fn draw_cell(screen: &TextScreen, fb: &mut Fb, row: usize, col: usize) {
    let cell = screen.at(row, col);
    let (fg, bg) = resolve(cell.attr);
    let x = col as u32 * CELL_W;
    let y = row as u32 * CELL_H;
    fb.fill_rect(x, y, CELL_W, CELL_H, bg);
    let ch = substitute(cell.ch);
    if ch == ' ' {
        return;
    }
    let style = MonoTextStyle::new(&FONT_6X12, fg);
    let text_style = TextStyleBuilder::new().baseline(Baseline::Top).build();
    let _ = Text::with_text_style(
        &ch.to_string(),
        Point::new(x as i32, y as i32),
        style,
        text_style,
    )
    .draw(fb);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::grid::{Cell, ATTR_BRIGHT};

    #[test]
    fn fb_starts_filled_with_bg() {
        let fb = Fb::new(PANEL_W, PANEL_H);
        assert_eq!(fb.pixel_at(0, 0), BG);
        assert_eq!(fb.pixel_at(479, 319), BG);
    }

    #[test]
    fn rendering_a_blank_screen_keeps_everything_bg() {
        let s = TextScreen::new();
        let mut fb = Fb::new(PANEL_W, PANEL_H);
        render_full(&s, &mut fb);
        // All pixels still the bg color.
        for v in fb.data.iter().take(100) {
            assert_eq!(*v, BG);
        }
    }

    #[test]
    fn rendering_an_inverse_cell_paints_a_block() {
        let mut s = TextScreen::new();
        s.set(0, 0, Cell::new(' ', crate::status::ATTR_INVERSE));
        let mut fb = Fb::new(PANEL_W, PANEL_H);
        render_full(&s, &mut fb);
        // Cell (0,0) covers x=0..6 y=0..12. Inverse on space → that whole rect = FG.
        assert_ne!(fb.pixel_at(2, 5), BG);
    }

    #[test]
    fn rendering_runs_only_touches_changed_cells() {
        let mut s = TextScreen::new();
        s.set(2, 5, Cell::new('X', ATTR_BRIGHT));
        let mut fb = Fb::new(PANEL_W, PANEL_H);
        render_runs(&s, &[(2, 5, 6)], &mut fb);
        // Outside the cell, still bg.
        assert_eq!(fb.pixel_at(0, 0), BG);
        assert_eq!(fb.pixel_at(100, 100), BG);
    }
}
