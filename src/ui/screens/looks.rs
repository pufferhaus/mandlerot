use crate::status::{Cell, TextScreen, ATTR_BRIGHT, ATTR_DIM, ATTR_INVERSE, ATTR_NORMAL};
use crate::ui::{RenderCtx, Screen, ScreenCtx, ScreenResult};

const N_SLOTS: u8 = 8;

pub struct LooksScreen {
    cursor: u8,
}

impl Default for LooksScreen {
    fn default() -> Self {
        Self { cursor: 0 }
    }
}

impl LooksScreen {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Screen for LooksScreen {
    fn render(&self, g: &mut TextScreen, ctx: &RenderCtx) {
        draw_border(g, "LOOKS");
        g.write(2, 3, ATTR_DIM, "SLOT  NAME              SAVED");
        let start_row = 4;
        let active = ctx.active_look_slot;
        for i in 0..N_SLOTS {
            let row = start_row + (i as usize) * 2;
            let slot = i + 1;
            let is_cursor = self.cursor == i;
            let attr = if is_cursor { ATTR_INVERSE } else { ATTR_NORMAL };
            let marker = if is_cursor { '>' } else { ' ' };
            g.set(row, 2, Cell::new(marker, ATTR_BRIGHT));
            g.write(row, 4, attr, &format!("{slot}"));
            let star = if active == Some(slot) { '*' } else { ' ' };
            g.set(row, 5, Cell::new(star, ATTR_BRIGHT));
            let entry = ctx.looks_view.and_then(|v| v.get(i as usize)).copied().flatten();
            match entry {
                Some((name, saved_at)) => {
                    g.write(row, 7, attr, &format!("{:<18}", truncate(name, 18)));
                    g.write(row, 26, ATTR_DIM, &short_saved(saved_at));
                }
                None => {
                    g.write(row, 7, ATTR_DIM, "(empty)");
                }
            }
        }
        draw_footer(g, "^v select   Ent recall   d delete   Esc back");
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
                if self.cursor + 1 < N_SLOTS {
                    self.cursor += 1;
                }
                ScreenResult::Continue
            }
            "Enter" | "NumpadEnter" => ScreenResult::RecallLook(self.cursor + 1),
            "d" | "D" | "Delete" => ScreenResult::DeleteLook(self.cursor + 1),
            _ => ScreenResult::Continue,
        }
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.chars().count() <= max {
        return s;
    }
    let mut iter = s.char_indices();
    let cut = iter.nth(max).map(|(i, _)| i).unwrap_or(s.len());
    &s[..cut]
}

fn short_saved(iso: &str) -> &str {
    if iso.len() >= 10 && iso.as_bytes()[4] == b'-' && iso.as_bytes()[7] == b'-' {
        &iso[..10]
    } else {
        iso
    }
}

fn draw_border(g: &mut TextScreen, title: &str) {
    g.fill(0, 0, 80, '\u{2500}', ATTR_DIM);
    g.set(0, 0, Cell::new('\u{250C}', ATTR_DIM));
    g.set(0, 79, Cell::new('\u{2510}', ATTR_DIM));
    g.write(0, 3, ATTR_BRIGHT, &format!(" {title} "));
    for r in 1..25 {
        g.set(r, 0, Cell::new('\u{2502}', ATTR_DIM));
        g.set(r, 79, Cell::new('\u{2502}', ATTR_DIM));
    }
    g.fill(25, 0, 80, '\u{2500}', ATTR_DIM);
    g.set(25, 0, Cell::new('\u{2514}', ATTR_DIM));
    g.set(25, 79, Cell::new('\u{2518}', ATTR_DIM));
}

fn draw_footer(g: &mut TextScreen, hint: &str) {
    g.fill(23, 0, 80, '\u{2500}', ATTR_DIM);
    g.set(23, 0, Cell::new('\u{251C}', ATTR_DIM));
    g.set(23, 79, Cell::new('\u{2524}', ATTR_DIM));
    g.write(24, 2, ATTR_DIM, hint);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::params::AudioParams;
    use crate::preset::{LookStore, SlotBindings};
    use crate::scene::{LoadedScene, SceneLibrary, SceneMeta};
    use crate::state::{BlendMode, SharedState};

    fn test_lib() -> SceneLibrary {
        let mut lib = SceneLibrary::default();
        for n in ["plasma", "solid"] {
            let meta = SceneMeta::parse(&format!("name = \"{n}\"\n"), "inline").unwrap();
            lib.upsert(
                n,
                LoadedScene {
                    meta,
                    fragment_body: "void main() {}".into(),
                    source_path: std::path::PathBuf::from("inline"),
                },
            );
        }
        lib
    }

    fn lib_state(lib: &SceneLibrary) -> SharedState {
        SharedState::from_initial(lib, "plasma", "solid", 0.0, BlendMode::Mix).unwrap()
    }

    fn s_ctx<'a>(
        scenes: &'a [String],
        bindings: &'a mut SlotBindings,
        dir: &'a std::path::Path,
        audio: &'a std::sync::Arc<AudioParams>,
        looks: &'a mut LookStore,
    ) -> ScreenCtx<'a> {
        ScreenCtx {
            scenes,
            bindings,
            state_dir: dir,
            audio,
            postfx: None,
            chromakey: None,
            video_status: crate::video::VideoStatus::NoDevice,
            active_look_slot: None,
            looks: Some(looks),
        }
    }

    #[test]
    fn down_moves_cursor_clamped() {
        let mut screen = LooksScreen::new();
        let tmp = tempfile::tempdir().unwrap();
        let lib = test_lib();
        let mut bindings = SlotBindings::default();
        let audio = AudioParams::new();
        let mut looks = LookStore::load_or_empty(&tmp.path().join("p.json")).unwrap();
        let scenes: Vec<String> = vec![];
        for _ in 0..20 {
            let mut ctx = s_ctx(&scenes, &mut bindings, tmp.path(), &audio, &mut looks);
            screen.handle_key("Down", &mut ctx);
        }
        assert_eq!(screen.cursor, 7);
    }

    #[test]
    fn enter_returns_recall_look_variant() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = test_lib();
        let mut state = lib_state(&lib);
        let mut bindings = SlotBindings::default();
        let audio = AudioParams::new();
        let mut looks = LookStore::load_or_empty(&tmp.path().join("p.json")).unwrap();
        looks.save(1, &state, Some("test".into())).unwrap();
        state.xfade = 0.9;
        let scenes: Vec<String> = vec![];
        let mut screen = LooksScreen::new();
        let mut ctx = s_ctx(&scenes, &mut bindings, tmp.path(), &audio, &mut looks);
        let result = screen.handle_key("Enter", &mut ctx);
        assert!(matches!(result, ScreenResult::RecallLook(1)));
    }

    #[test]
    fn d_returns_delete_look_variant() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = test_lib();
        let state = lib_state(&lib);
        let mut bindings = SlotBindings::default();
        let audio = AudioParams::new();
        let mut looks = LookStore::load_or_empty(&tmp.path().join("p.json")).unwrap();
        looks.save(1, &state, Some("test".into())).unwrap();
        let scenes: Vec<String> = vec![];
        let mut screen = LooksScreen::new();
        let mut ctx = s_ctx(&scenes, &mut bindings, tmp.path(), &audio, &mut looks);
        let result = screen.handle_key("d", &mut ctx);
        assert!(matches!(result, ScreenResult::DeleteLook(1)));
    }
}
