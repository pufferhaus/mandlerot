//! Root menu opened by F4. Lists the available settings categories;
//! Enter pushes the child screen for the selected entry.
//!
//! Adding a new category = one more `Entry` variant + a `Push` arm in
//! `enter()`. The screen layout adapts to the entry count automatically.

use crate::status::{Cell, TextScreen, ATTR_BRIGHT, ATTR_DIM, ATTR_INVERSE, ATTR_NORMAL};
use crate::ui::screens::{AudioSettingsScreen, SlotsScreen};
use crate::ui::{RenderCtx, Screen, ScreenCtx, ScreenResult};

#[derive(Clone, Copy)]
struct Entry {
    label: &'static str,
    hint: &'static str,
}

const ENTRIES: &[Entry] = &[
    Entry {
        label: "Preferences",
        hint: "Misc app preferences (coming soon)",
    },
    Entry {
        label: "Audio",
        hint: "Noise floor + per-band gain",
    },
    Entry {
        label: "Slot Mapper",
        hint: "Bind 1..9 keys to scenes",
    },
];

pub struct SettingsScreen {
    cursor: u8,
}

impl Default for SettingsScreen {
    fn default() -> Self {
        Self { cursor: 0 }
    }
}

impl SettingsScreen {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Screen for SettingsScreen {
    fn render(&self, g: &mut TextScreen, _ctx: &RenderCtx) {
        draw_border(g, "SETTINGS");
        let start_row = 5;
        for (i, e) in ENTRIES.iter().enumerate() {
            let row = start_row + i * 2;
            let is_cursor = self.cursor as usize == i;
            let attr = if is_cursor { ATTR_INVERSE } else { ATTR_NORMAL };
            let marker = if is_cursor { '>' } else { ' ' };
            g.set(row, 3, Cell::new(marker, ATTR_BRIGHT));
            let n_char = std::char::from_digit((i + 1) as u32, 10).unwrap_or('?');
            g.set(row, 4, Cell::new(n_char, attr));
            g.set(row, 5, Cell::new('.', attr));
            g.write(row, 7, attr, e.label);
            g.write(row + 1, 7, ATTR_DIM, e.hint);
        }
        draw_footer(g, "1-3 / Enter open   ^v cursor   Esc back");
    }

    fn handle_key(&mut self, key: &str, _ctx: &mut ScreenCtx) -> ScreenResult {
        match key {
            "Esc" | "Backspace" => ScreenResult::Pop,
            "Up" => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                ScreenResult::Continue
            }
            "Down" => {
                if (self.cursor as usize) + 1 < ENTRIES.len() {
                    self.cursor += 1;
                }
                ScreenResult::Continue
            }
            "Enter" | "NumpadEnter" => enter(self.cursor),
            d if is_entry_digit(d) => {
                let n = digit_value(d) - 1;
                self.cursor = n;
                enter(n)
            }
            _ => ScreenResult::Continue,
        }
    }
}

fn enter(idx: u8) -> ScreenResult {
    match idx {
        0 => ScreenResult::Continue, // Preferences — stub
        1 => ScreenResult::Push(Box::new(AudioSettingsScreen::new())),
        2 => ScreenResult::Push(Box::new(SlotsScreen::new())),
        _ => ScreenResult::Continue,
    }
}

fn is_entry_digit(key: &str) -> bool {
    let n = digit_value(key);
    n >= 1 && (n as usize) <= ENTRIES.len()
}

fn digit_value(key: &str) -> u8 {
    if key.len() == 1 {
        return key.as_bytes()[0].wrapping_sub(b'0');
    }
    if let Some(rest) = key.strip_prefix("Numpad") {
        if rest.len() == 1 && rest.as_bytes()[0].is_ascii_digit() {
            return rest.as_bytes()[0] - b'0';
        }
    }
    0
}

fn draw_border(g: &mut TextScreen, title: &str) {
    g.fill(0, 0, 80, '─', ATTR_DIM);
    g.set(0, 0, Cell::new('┌', ATTR_DIM));
    g.set(0, 79, Cell::new('┐', ATTR_DIM));
    g.write(0, 3, ATTR_BRIGHT, &format!(" {title} "));
    for r in 1..25 {
        g.set(r, 0, Cell::new('│', ATTR_DIM));
        g.set(r, 79, Cell::new('│', ATTR_DIM));
    }
    g.fill(25, 0, 80, '─', ATTR_DIM);
    g.set(25, 0, Cell::new('└', ATTR_DIM));
    g.set(25, 79, Cell::new('┘', ATTR_DIM));
}

fn draw_footer(g: &mut TextScreen, hint: &str) {
    g.fill(23, 0, 80, '─', ATTR_DIM);
    g.set(23, 0, Cell::new('├', ATTR_DIM));
    g.set(23, 79, Cell::new('┤', ATTR_DIM));
    g.write(24, 2, ATTR_DIM, hint);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::SlotBindings;

    fn audio_for_test() -> std::sync::Arc<crate::audio::params::AudioParams> {
        crate::audio::params::AudioParams::new()
    }
    fn s_ctx<'a>(
        scenes: &'a [String],
        bindings: &'a mut SlotBindings,
        dir: &'a std::path::Path,
        audio: &'a std::sync::Arc<crate::audio::params::AudioParams>,
    ) -> ScreenCtx<'a> {
        ScreenCtx {
            scenes,
            bindings,
            state_dir: dir,
            audio,
        }
    }
    fn r_ctx<'a>(
        scenes: &'a [String],
        bindings: &'a SlotBindings,
        audio: &'a std::sync::Arc<crate::audio::params::AudioParams>,
    ) -> RenderCtx<'a> {
        RenderCtx {
            scenes,
            bindings,
            audio,
        }
    }

    #[test]
    fn renders_three_entries() {
        let s = SettingsScreen::new();
        let mut g = TextScreen::new();
        let b = SlotBindings::default();
        let scenes: Vec<String> = vec![];
        let audio = audio_for_test();
        s.render(&mut g, &r_ctx(&scenes, &b, &audio));
        let row5: String = (7..18).map(|c| g.at(5, c).ch).collect();
        assert!(row5.starts_with("Preferences"));
        let row7: String = (7..18).map(|c| g.at(7, c).ch).collect();
        assert!(row7.starts_with("Audio"));
        let row9: String = (7..18).map(|c| g.at(9, c).ch).collect();
        assert!(row9.starts_with("Slot Mapper"));
    }

    #[test]
    fn pressing_three_pushes_slot_mapper() {
        let mut s = SettingsScreen::new();
        let scenes: Vec<String> = vec![];
        let mut b = SlotBindings::default();
        let dir = std::path::PathBuf::from("/tmp");
        let audio = audio_for_test();
        let r = s.handle_key("3", &mut s_ctx(&scenes, &mut b, &dir, &audio));
        assert!(matches!(r, ScreenResult::Push(_)));
        assert_eq!(s.cursor, 2);
    }

    #[test]
    fn pressing_one_preferences_is_currently_continue() {
        let mut s = SettingsScreen::new();
        let scenes: Vec<String> = vec![];
        let mut b = SlotBindings::default();
        let dir = std::path::PathBuf::from("/tmp");
        let audio = audio_for_test();
        let r = s.handle_key("1", &mut s_ctx(&scenes, &mut b, &dir, &audio));
        assert!(matches!(r, ScreenResult::Continue));
    }

    #[test]
    fn esc_pops() {
        let mut s = SettingsScreen::new();
        let scenes: Vec<String> = vec![];
        let mut b = SlotBindings::default();
        let dir = std::path::PathBuf::from("/tmp");
        let audio = audio_for_test();
        let r = s.handle_key("Esc", &mut s_ctx(&scenes, &mut b, &dir, &audio));
        assert!(matches!(r, ScreenResult::Pop));
    }
}
