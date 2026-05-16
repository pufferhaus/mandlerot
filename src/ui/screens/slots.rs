//! Top-level slot binding view. Shows the 9 slot keys and their current
//! bindings (explicit or "auto:<alphabetical-fallback>"). Pressing 1..9
//! pushes a SceneListScreen for that slot. `0` clears the focused slot.

use crate::preset::resolve_slot;
use crate::status::{Cell, TextScreen, ATTR_BRIGHT, ATTR_DIM, ATTR_INVERSE, ATTR_NORMAL};
use crate::ui::screens::SceneListScreen;
use crate::ui::{RenderCtx, Screen, ScreenCtx, ScreenResult};

pub struct SlotsScreen {
    /// 0-based index into the 9 rows.
    cursor: u8,
}

impl Default for SlotsScreen {
    fn default() -> Self {
        Self { cursor: 0 }
    }
}

impl SlotsScreen {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Screen for SlotsScreen {
    fn render(&self, g: &mut TextScreen, ctx: &RenderCtx) {
        draw_border(g, "SLOTS");
        let start_row = 4;
        for i in 0..9u8 {
            let row = start_row + i as usize;
            let n = i + 1;
            let is_cursor = self.cursor == i;
            let prefix_attr = if is_cursor { ATTR_INVERSE } else { ATTR_NORMAL };
            let cursor_ch = if is_cursor { '>' } else { ' ' };
            g.set(row, 3, Cell::new(cursor_ch, ATTR_BRIGHT));
            g.set(
                row,
                4,
                Cell::new(
                    std::char::from_digit(n as u32, 10).unwrap_or('?'),
                    prefix_attr,
                ),
            );
            g.set(row, 5, Cell::new(':', prefix_attr));

            let bound = ctx.bindings.get(n);
            let resolved = resolve_slot(ctx.bindings, ctx.scenes, n).unwrap_or("(no scenes)");
            let label = match bound {
                Some(name) => name.to_string(),
                None => format!("(auto: {resolved})"),
            };
            let label_attr = if bound.is_some() {
                ATTR_BRIGHT
            } else {
                ATTR_DIM
            };
            g.write(row, 8, label_attr, &truncate(&label, 60));
        }
        draw_footer(g, "1-9 pick   0 clear   ^v cursor   Enter open   Esc back");
    }

    fn handle_key(&mut self, key: &str, ctx: &mut ScreenCtx) -> ScreenResult {
        match key {
            "Esc" | "Backspace" => ScreenResult::Pop,
            "Up" => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                ScreenResult::Continue
            }
            "Down" => {
                if self.cursor < 8 {
                    self.cursor += 1;
                }
                ScreenResult::Continue
            }
            "Enter" | "NumpadEnter" => ScreenResult::Push(Box::new(SceneListScreen::for_slot(
                self.cursor + 1,
                ctx.scenes,
            ))),
            "0" | "Numpad0" => {
                let slot = self.cursor + 1;
                ctx.bindings.set(slot, None);
                if let Err(e) = ctx.bindings.save(ctx.state_dir) {
                    tracing::warn!("save slots.toml: {e}");
                }
                ScreenResult::Continue
            }
            d if is_slot_digit(d) => {
                let n: u8 = digit_value(d);
                self.cursor = n - 1;
                ScreenResult::Push(Box::new(SceneListScreen::for_slot(n, ctx.scenes)))
            }
            _ => ScreenResult::Continue,
        }
    }
}

fn is_slot_digit(key: &str) -> bool {
    let n = digit_value(key);
    (1..=9).contains(&n)
}

fn digit_value(key: &str) -> u8 {
    if key.len() == 1 {
        return key.as_bytes()[0].wrapping_sub(b'0');
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

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

#[cfg(test)]
pub fn cursor_of(s: &SlotsScreen) -> u8 {
    s.cursor
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::SlotBindings;

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
            postfx: None,
            video_status: crate::video::VideoStatus::NoDevice,
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
            postfx: None,
            filtered_scenes: 0,
            pi_gen: crate::platform::PiGen::Unknown,
            video_status: crate::video::VideoStatus::NoDevice,
        }
    }

    fn audio_for_test() -> std::sync::Arc<crate::audio::params::AudioParams> {
        crate::audio::params::AudioParams::new()
    }

    #[test]
    fn down_advances_cursor_clamping_at_eight() {
        let mut s = SlotsScreen::new();
        let scenes: Vec<String> = vec![];
        let mut b = SlotBindings::default();
        let dir = std::path::PathBuf::from("/tmp");
        let audio = audio_for_test();
        for _ in 0..20 {
            let _ = s.handle_key("Down", &mut s_ctx(&scenes, &mut b, &dir, &audio));
        }
        assert_eq!(cursor_of(&s), 8);
    }

    #[test]
    fn pressing_digit_pushes_scene_list_and_moves_cursor() {
        let mut s = SlotsScreen::new();
        let scenes: Vec<String> = vec!["a".into(), "b".into()];
        let mut b = SlotBindings::default();
        let dir = std::path::PathBuf::from("/tmp");
        let audio = audio_for_test();
        let r = s.handle_key("3", &mut s_ctx(&scenes, &mut b, &dir, &audio));
        assert!(matches!(r, ScreenResult::Push(_)));
        assert_eq!(cursor_of(&s), 2);
    }

    #[test]
    fn zero_clears_binding_at_cursor() {
        let mut s = SlotsScreen::new();
        let scenes: Vec<String> = vec![];
        let mut b = SlotBindings::default();
        b.set(1, Some("foo".into()));
        let dir = tempfile::tempdir().unwrap();
        let audio = audio_for_test();
        let _ = s.handle_key("0", &mut s_ctx(&scenes, &mut b, dir.path(), &audio));
        assert_eq!(b.get(1), None);
    }

    #[test]
    fn esc_pops() {
        let mut s = SlotsScreen::new();
        let scenes: Vec<String> = vec![];
        let mut b = SlotBindings::default();
        let dir = std::path::PathBuf::from("/tmp");
        let audio = audio_for_test();
        let r = s.handle_key("Esc", &mut s_ctx(&scenes, &mut b, &dir, &audio));
        assert!(matches!(r, ScreenResult::Pop));
    }

    #[test]
    fn render_shows_explicit_binding_label() {
        let s = SlotsScreen::new();
        let mut g = TextScreen::new();
        let mut b = SlotBindings::default();
        b.set(1, Some("plasma".into()));
        let scenes = vec!["plasma".to_string(), "solid".into()];
        let audio = audio_for_test();
        s.render(&mut g, &r_ctx(&scenes, &b, &audio));
        let label: String = (8..14).map(|c| g.at(4, c).ch).collect();
        assert_eq!(label, "plasma");
    }

    #[test]
    fn numpad_digit_does_not_trigger_slot_jump() {
        let mut s = SlotsScreen::new();
        let scenes: Vec<String> = vec!["a".into(), "b".into()];
        let mut b = SlotBindings::default();
        let dir = std::path::PathBuf::from("/tmp");
        let audio = audio_for_test();
        let r = s.handle_key("Numpad3", &mut s_ctx(&scenes, &mut b, &dir, &audio));
        assert!(matches!(r, ScreenResult::Continue), "Numpad3 must not push");
        assert_eq!(cursor_of(&s), 0, "Numpad3 must not move the cursor");
    }

    #[test]
    fn render_shows_auto_label_for_unbound() {
        let s = SlotsScreen::new();
        let mut g = TextScreen::new();
        let b = SlotBindings::default();
        let scenes = vec!["alpha".to_string(), "beta".into()];
        let audio = audio_for_test();
        s.render(&mut g, &r_ctx(&scenes, &b, &audio));
        let row: String = (8..30).map(|c| g.at(4, c).ch).collect();
        assert!(row.starts_with("(auto:"), "row was: {row:?}");
    }
}
