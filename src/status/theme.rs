//! Amber phosphor color palette as Rgb565 values.

use embedded_graphics::pixelcolor::Rgb565;

use crate::status::grid::{ATTR_BRIGHT, ATTR_DIM, ATTR_INVERSE};

// Color values are tuned empirically against the MPI3501 ILI9486 SPI panel.
// The R channel on this clone is weak so G has to carry brightness — and
// at low R values G dominates entirely (text reads as green). We cap R at
// max and use only G to vary "intensity"; dim is just pure red as a
// visually distinct accent rather than a dimmer-amber.
/// Body
pub const FG_NORMAL: Rgb565 = Rgb565::new(31, 36, 0);
/// Bright
pub const FG_BRIGHT: Rgb565 = Rgb565::new(31, 48, 0);
/// Dim — pure red. Don't try to dim with reduced R; the hue collapses to green.
pub const FG_DIM: Rgb565 = Rgb565::new(31, 0, 0);
/// Background — pure black so the panel BG isn't tinted by the G bleed.
pub const BG: Rgb565 = Rgb565::new(0, 0, 0);

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
