//! Amber phosphor color palette as Rgb565 values.

use embedded_graphics::pixelcolor::Rgb565;

use crate::status::grid::{ATTR_BRIGHT, ATTR_DIM, ATTR_INVERSE};

/// Body (#FFB000)
pub const FG_NORMAL: Rgb565 = Rgb565::new(31, 44, 0);
/// Bright (#FFD000)
pub const FG_BRIGHT: Rgb565 = Rgb565::new(31, 52, 0);
/// Dim (#663D00)
pub const FG_DIM: Rgb565 = Rgb565::new(12, 15, 0);
/// Background (#0A0500)
pub const BG: Rgb565 = Rgb565::new(1, 1, 0);

/// Resolve `(fg, bg)` for a given attribute byte.
pub fn resolve(attr: u8) -> (Rgb565, Rgb565) {
    let bright = attr & ATTR_BRIGHT != 0;
    let dim = attr & ATTR_DIM != 0;
    let inverse = attr & ATTR_INVERSE != 0;
    let fg = if dim {
        FG_DIM
    } else if bright {
        FG_BRIGHT
    } else {
        FG_NORMAL
    };
    if inverse {
        (BG, fg)
    } else {
        (fg, BG)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_is_fg_on_bg() {
        let (fg, bg) = resolve(0);
        assert_eq!(fg, FG_NORMAL);
        assert_eq!(bg, BG);
    }

    #[test]
    fn bright_swaps_in_yellow() {
        let (fg, _) = resolve(ATTR_BRIGHT);
        assert_eq!(fg, FG_BRIGHT);
    }

    #[test]
    fn inverse_swaps_fg_and_bg() {
        let (fg, bg) = resolve(ATTR_INVERSE);
        assert_eq!(fg, BG);
        assert_eq!(bg, FG_NORMAL);
    }

    #[test]
    fn dim_overrides_bright() {
        let (fg, _) = resolve(ATTR_BRIGHT | ATTR_DIM);
        assert_eq!(fg, FG_DIM);
    }
}
