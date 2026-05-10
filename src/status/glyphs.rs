//! Glyph lookup. We use embedded-graphics' built-in `FONT_6X12` for ASCII and
//! substitute simple ASCII for the box-drawing characters our compose layer
//! emits — the panel fits 80×26 cells exactly, and the substitutions read
//! perfectly clearly on the actual 480×320 panel.

pub const CELL_W: u32 = 6;
pub const CELL_H: u32 = 12;

/// Map a logical char to whatever printable ASCII char will actually be
/// rendered. Box-drawing → `-`, `|`, `+`, `=`. Filled-block `█` → `#`.
/// Mid-dot `·` → `.`. Identity for everything else.
pub fn substitute(ch: char) -> char {
    match ch {
        '─' | '━' => '-',
        '│' | '┃' => '|',
        '┌' | '┐' | '└' | '┘' | '├' | '┤' | '┬' | '┴' | '┼' => '+',
        '═' => '=',
        '█' | '▌' | '▐' | '▀' | '▄' => '#',
        '·' => '.',
        '↺' => '~',
        '→' => '>',
        '←' => '<',
        c if c.is_ascii() => c,
        _ => '?',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_drawing_substitutes_to_ascii() {
        assert_eq!(substitute('─'), '-');
        assert_eq!(substitute('│'), '|');
        assert_eq!(substitute('┌'), '+');
        assert_eq!(substitute('┴'), '+');
        assert_eq!(substitute('█'), '#');
    }

    #[test]
    fn ascii_passes_through() {
        assert_eq!(substitute('A'), 'A');
        assert_eq!(substitute(' '), ' ');
        assert_eq!(substitute('5'), '5');
    }

    #[test]
    fn non_ascii_fallback_is_question_mark() {
        assert_eq!(substitute('é'), '?');
        assert_eq!(substitute('🎵'), '?');
    }
}
