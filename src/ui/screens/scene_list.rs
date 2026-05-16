//! Scrollable picker: a list of all loaded scenes. Selecting one writes
//! the binding for `for_slot` and pops back to the SlotsScreen.

use crate::status::{Cell, TextScreen, ATTR_BRIGHT, ATTR_DIM, ATTR_INVERSE, ATTR_NORMAL};
use crate::ui::{RenderCtx, Screen, ScreenCtx, ScreenResult};

const VISIBLE_ROWS: usize = 18;

pub struct SceneListScreen {
    for_slot: u8,
    /// Cached at construction so resize doesn't shift selection unexpectedly.
    /// On commit we re-resolve through ctx.scenes to use the live name list.
    cursor: usize,
    scroll: usize,
}

impl SceneListScreen {
    pub fn for_slot(slot: u8, scenes: &[String]) -> Self {
        let _ = scenes; // selection starts at top regardless
        Self {
            for_slot: slot,
            cursor: 0,
            scroll: 0,
        }
    }
}

impl Screen for SceneListScreen {
    fn render(&self, g: &mut TextScreen, ctx: &RenderCtx) {
        let title = format!("PICK SCENE -> slot {}", self.for_slot);
        draw_border(g, &title);

        // Filter summary: only paint when at least one scene was dropped.
        if ctx.filtered_scenes > 0 {
            let line = format!(
                "{} visible / {} hidden on {}",
                ctx.scenes.len(),
                ctx.filtered_scenes,
                ctx.pi_gen.as_str(),
            );
            g.write(2, 3, ATTR_DIM, &line);
        }

        if ctx.scenes.is_empty() {
            g.write(6, 4, ATTR_DIM, "(no scenes available)");
            draw_footer(g, "Esc back");
            return;
        }

        // Scroll into view if cursor moved off-screen.
        let scroll = clamp_scroll(self.scroll, self.cursor, ctx.scenes.len());
        for row_idx in 0..VISIBLE_ROWS {
            let i = scroll + row_idx;
            if i >= ctx.scenes.len() {
                break;
            }
            let row = 4 + row_idx;
            let is_cursor = i == self.cursor;
            let attr = if is_cursor { ATTR_INVERSE } else { ATTR_NORMAL };
            let marker = if is_cursor { '>' } else { ' ' };
            g.set(row, 3, Cell::new(marker, ATTR_BRIGHT));
            g.write(row, 5, attr, &truncate(&ctx.scenes[i], 70));
        }

        // Scroll indicator on the right margin.
        if ctx.scenes.len() > VISIBLE_ROWS {
            let total = ctx.scenes.len();
            // map cursor position to a track row
            let track_top = 4;
            let track_h = VISIBLE_ROWS;
            let pos = ((self.cursor as f32 / (total - 1).max(1) as f32) * (track_h - 1) as f32)
                .round() as usize;
            g.set(track_top + pos.min(track_h - 1), 77, Cell::new('█', ATTR_BRIGHT));
        }

        draw_footer(g, "^v select   PgUp/PgDn page   Enter bind   Esc back");
    }

    fn handle_key(&mut self, key: &str, ctx: &mut ScreenCtx) -> ScreenResult {
        let n = ctx.scenes.len();
        if n == 0 {
            if matches!(key, "Esc" | "Backspace") {
                return ScreenResult::Pop;
            }
            return ScreenResult::Continue;
        }
        match key {
            "Esc" | "Backspace" => ScreenResult::Pop,
            "Up" => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                self.scroll = clamp_scroll(self.scroll, self.cursor, n);
                ScreenResult::Continue
            }
            "Down" => {
                if self.cursor + 1 < n {
                    self.cursor += 1;
                }
                self.scroll = clamp_scroll(self.scroll, self.cursor, n);
                ScreenResult::Continue
            }
            "PageUp" => {
                self.cursor = self.cursor.saturating_sub(VISIBLE_ROWS);
                self.scroll = clamp_scroll(self.scroll, self.cursor, n);
                ScreenResult::Continue
            }
            "PageDown" => {
                self.cursor = (self.cursor + VISIBLE_ROWS).min(n - 1);
                self.scroll = clamp_scroll(self.scroll, self.cursor, n);
                ScreenResult::Continue
            }
            "Home" => {
                self.cursor = 0;
                self.scroll = 0;
                ScreenResult::Continue
            }
            "End" => {
                self.cursor = n - 1;
                self.scroll = clamp_scroll(self.scroll, self.cursor, n);
                ScreenResult::Continue
            }
            "Enter" | "NumpadEnter" => {
                let pick = ctx.scenes[self.cursor].clone();
                ctx.bindings.set(self.for_slot, Some(pick));
                if let Err(e) = ctx.bindings.save(ctx.state_dir) {
                    tracing::warn!("save slots.toml: {e}");
                }
                ScreenResult::Pop
            }
            _ => ScreenResult::Continue,
        }
    }
}

fn clamp_scroll(scroll: usize, cursor: usize, total: usize) -> usize {
    let mut s = scroll;
    if cursor < s {
        s = cursor;
    } else if cursor >= s + VISIBLE_ROWS {
        s = cursor + 1 - VISIBLE_ROWS;
    }
    let max_scroll = total.saturating_sub(VISIBLE_ROWS);
    s.min(max_scroll)
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
mod tests {
    use super::*;
    use crate::preset::SlotBindings;

    fn audio_for_test() -> std::sync::Arc<crate::audio::params::AudioParams> {
        crate::audio::params::AudioParams::new()
    }
    fn ctx_s<'a>(
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
    fn ctx_r<'a>(
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

    #[test]
    fn down_advances_clamping_at_end() {
        let scenes: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
        let mut s = SceneListScreen::for_slot(1, &scenes);
        let mut b = SlotBindings::default();
        let dir = std::path::PathBuf::from("/tmp");
        let audio = audio_for_test();
        for _ in 0..10 {
            let _ = s.handle_key("Down", &mut ctx_s(&scenes, &mut b, &dir, &audio));
        }
        assert_eq!(s.cursor, 2);
    }

    #[test]
    fn enter_writes_binding_and_pops() {
        let scenes: Vec<String> = vec!["plasma".into(), "solid".into(), "menger".into()];
        let mut s = SceneListScreen::for_slot(4, &scenes);
        let mut b = SlotBindings::default();
        let dir = tempfile::tempdir().unwrap();
        let audio = audio_for_test();
        let _ = s.handle_key("Down", &mut ctx_s(&scenes, &mut b, dir.path(), &audio));
        let r = s.handle_key("Enter", &mut ctx_s(&scenes, &mut b, dir.path(), &audio));
        assert!(matches!(r, ScreenResult::Pop));
        assert_eq!(b.get(4), Some("solid"));
    }

    #[test]
    fn escape_pops_without_writing() {
        let scenes: Vec<String> = vec!["plasma".into(), "solid".into()];
        let mut s = SceneListScreen::for_slot(2, &scenes);
        let mut b = SlotBindings::default();
        let dir = std::path::PathBuf::from("/tmp");
        let audio = audio_for_test();
        let r = s.handle_key("Esc", &mut ctx_s(&scenes, &mut b, &dir, &audio));
        assert!(matches!(r, ScreenResult::Pop));
        assert_eq!(b.get(2), None);
    }

    #[test]
    fn empty_scene_list_renders_placeholder() {
        let s = SceneListScreen::for_slot(1, &[]);
        let mut g = TextScreen::new();
        let scenes: Vec<String> = vec![];
        let b = SlotBindings::default();
        let audio = audio_for_test();
        s.render(&mut g, &ctx_r(&scenes, &b, &audio));
        let row: String = (4..30).map(|c| g.at(6, c).ch).collect();
        assert!(row.starts_with("(no scenes"));
    }

    #[test]
    fn scroll_follows_cursor_past_visible_window() {
        let mut scenes: Vec<String> = Vec::new();
        for i in 0..40 {
            scenes.push(format!("scene{i:02}"));
        }
        let mut s = SceneListScreen::for_slot(1, &scenes);
        let mut b = SlotBindings::default();
        let dir = std::path::PathBuf::from("/tmp");
        let audio = audio_for_test();
        for _ in 0..25 {
            let _ = s.handle_key("Down", &mut ctx_s(&scenes, &mut b, &dir, &audio));
        }
        assert!(s.scroll > 0);
        // first visible row should be at or below cursor - VISIBLE_ROWS + 1
        assert!(s.scroll <= s.cursor);
        assert!(s.cursor < s.scroll + VISIBLE_ROWS);
    }
}
