//! Post-FX list + per-pass param drawer.
//!
//! `PostFxScreen` lists every pass loaded from `postfx/`. The user toggles
//! `enabled` with Space/Enter, and Right (or Tab) dives into
//! `PostFxParamScreen` for the highlighted pass to nudge its 8 params.
//!
//! Every mutation is persisted to `<state_dir>/postfx.toml` immediately so
//! the on-disk chain state always reflects the live render. Mirrors the
//! eager-save pattern from `AudioSettingsScreen` and `SlotsScreen`.

use crate::status::{Cell, TextScreen, ATTR_BRIGHT, ATTR_DIM, ATTR_INVERSE, ATTR_NORMAL};
use crate::ui::{RenderCtx, Screen, ScreenCtx, ScreenResult};

/// Top-level pass list. Cursor selects a pass.
pub struct PostFxScreen {
    cursor: usize,
}

impl Default for PostFxScreen {
    fn default() -> Self {
        Self { cursor: 0 }
    }
}

impl PostFxScreen {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Screen for PostFxScreen {
    fn render(&self, g: &mut TextScreen, ctx: &RenderCtx) {
        draw_border(g, "POST-FX");
        let Some(pfx) = ctx.postfx else {
            g.write(4, 3, ATTR_DIM, "(post-fx unavailable in this context)");
            draw_footer(g, "Esc back");
            return;
        };
        let passes = pfx.passes();
        if passes.is_empty() {
            g.write(4, 3, ATTR_DIM, "No post-FX passes loaded.");
            g.write(5, 3, ATTR_DIM, "Drop `<name>.{glsl,toml}` pairs into `postfx/`.");
            draw_footer(g, "Esc back");
            return;
        }
        g.write(3, 3, ATTR_DIM, "PASS              ENABLED   PARAMS");
        let start_row = 5;
        for (i, p) in passes.iter().enumerate() {
            let row = start_row + i;
            if row > 21 {
                break;
            }
            let is_cursor = self.cursor == i;
            let attr = if is_cursor { ATTR_INVERSE } else { ATTR_NORMAL };
            let marker = if is_cursor { '>' } else { ' ' };
            g.set(row, 2, Cell::new(marker, ATTR_BRIGHT));
            // Name (padded to 16) + on/off + first param summary.
            let display = p
                .meta
                .display_name
                .clone()
                .unwrap_or_else(|| p.name.clone());
            g.write(row, 4, attr, &format!("{:<16}", truncate(&display, 16)));
            let on_str = if p.enabled { "[x]" } else { "[ ]" };
            let on_attr = if p.enabled { ATTR_BRIGHT } else { ATTR_DIM };
            g.write(row, 22, on_attr, on_str);
            // Summarise the first 3 params so the user can see what's set at
            // a glance without diving in.
            let mut col = 28usize;
            for (slot_i, d) in p.params.defs().iter().enumerate().take(3) {
                if col + 14 > 78 {
                    break;
                }
                if let Some(v) = p.params.get(&d.name) {
                    let name = truncate(&d.name, 6);
                    let summary = format!("{name}={}", short_val(v));
                    g.write(row, col, ATTR_DIM, &summary);
                    col += summary.chars().count() + 2;
                }
                let _ = slot_i;
            }
        }
        // Bind line
        let bind_row = 22usize;
        g.fill(bind_row, 2, 76, '─', ATTR_DIM);
        let label = match ctx.bound_state {
            None => "Bind: no active Look".to_string(),
            Some((slot, false, _)) => format!("[ ] Bind to Look {slot}"),
            Some((slot, true, false)) => format!("[ ] Bind to Look {slot} (paused)"),
            Some((slot, true, true)) => format!("[X] Bound to Look {slot}"),
        };
        let attr = if ctx.bound_state.is_none() { ATTR_DIM } else { ATTR_BRIGHT };
        g.write(bind_row + 1, 4, attr, &label);
        draw_footer(g, "^v select   Spc toggle   > tune   b bind   Esc back");
    }

    fn handle_key(&mut self, key: &str, ctx: &mut ScreenCtx) -> ScreenResult {
        let Some(pfx) = ctx.postfx.as_deref_mut() else {
            return ScreenResult::Pop;
        };
        let n = pfx.passes().len();
        if n == 0 {
            return match key {
                "Esc" | "Backspace" => ScreenResult::Pop,
                _ => ScreenResult::Continue,
            };
        }
        match key {
            "Esc" | "Backspace" => ScreenResult::Pop,
            "Up" => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                ScreenResult::Continue
            }
            "Down" => {
                if self.cursor + 1 < n {
                    self.cursor += 1;
                }
                ScreenResult::Continue
            }
            "Space" | "Enter" | "NumpadEnter" => {
                pfx.toggle(self.cursor);
                if let Err(e) = pfx.save_state(ctx.state_dir) {
                    tracing::warn!("save postfx.toml: {e}");
                }
                sync_active_look(ctx.looks.as_deref_mut(), ctx.active_look_slot, pfx);
                ScreenResult::Continue
            }
            "Right" | "Tab" => ScreenResult::Push(Box::new(PostFxParamScreen::new(self.cursor))),
            "b" | "B" => {
                let Some(slot) = ctx.active_look_slot else {
                    tracing::info!("postfx bind: no active Look");
                    return ScreenResult::Continue;
                };
                let Some(looks) = ctx.looks.as_deref_mut() else {
                    return ScreenResult::Continue;
                };
                let has = looks.has_snapshot(slot);
                let active = looks.is_bound_active(slot);
                if !has {
                    // First bind: capture + active=true
                    let snap = pfx.snapshot();
                    if let Err(e) = looks.save_postfx_snapshot(slot, snap) {
                        tracing::warn!("postfx bind: {e}");
                    }
                } else if !active {
                    // Paused -> active + restore
                    if let Err(e) = looks.set_postfx_active(slot, true) {
                        tracing::warn!("postfx bind: {e}");
                    }
                    if let Some(snap) = looks
                        .file
                        .slots
                        .get(&slot.to_string())
                        .and_then(|l| l.postfx.as_ref())
                        .cloned()
                    {
                        pfx.apply_snapshot(&snap);
                    }
                } else {
                    // Active -> paused
                    if let Err(e) = looks.set_postfx_active(slot, false) {
                        tracing::warn!("postfx bind: {e}");
                    }
                }
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }
}

/// Per-pass param drawer. Up/Down picks a param; Left/Right nudges its
/// value by 2% of `(max - min)`; `r` resets to the default.
pub struct PostFxParamScreen {
    pass_idx: usize,
    cursor: usize,
}

impl PostFxParamScreen {
    pub fn new(pass_idx: usize) -> Self {
        Self {
            pass_idx,
            cursor: 0,
        }
    }
}

impl Screen for PostFxParamScreen {
    fn render(&self, g: &mut TextScreen, ctx: &RenderCtx) {
        let title = ctx
            .postfx
            .and_then(|p| p.passes().get(self.pass_idx))
            .map(|p| {
                p.meta
                    .display_name
                    .clone()
                    .unwrap_or_else(|| p.name.clone())
            })
            .unwrap_or_else(|| "POST-FX PARAM".to_string());
        draw_border(g, &format!("POST-FX · {title}"));
        let Some(pfx) = ctx.postfx else {
            g.write(4, 3, ATTR_DIM, "(post-fx unavailable)");
            draw_footer(g, "Esc back");
            return;
        };
        let Some(p) = pfx.passes().get(self.pass_idx) else {
            g.write(4, 3, ATTR_DIM, "(pass missing)");
            draw_footer(g, "Esc back");
            return;
        };
        let defs = p.params.defs();
        if defs.is_empty() {
            g.write(4, 3, ATTR_DIM, "This pass has no params.");
            draw_footer(g, "Esc back");
            return;
        }
        let start_row = 4;
        for (i, d) in defs.iter().enumerate() {
            let row = start_row + i * 2;
            if row > 22 {
                break;
            }
            let is_cursor = self.cursor == i;
            let attr = if is_cursor { ATTR_INVERSE } else { ATTR_NORMAL };
            let marker = if is_cursor { '>' } else { ' ' };
            g.set(row, 3, Cell::new(marker, ATTR_BRIGHT));
            let name = truncate(&d.name, 14);
            g.write(row, 5, attr, &format!("{name:<14}"));
            let v = p.params.get(&d.name).unwrap_or(d.default);
            let frac = ((v - d.min) / (d.max - d.min)).clamp(0.0, 1.0);
            let bar_w = 28usize;
            let filled = (frac * bar_w as f32).round() as usize;
            g.set(row, 22, Cell::new('[', ATTR_DIM));
            for c in 0..bar_w {
                let ch = if c < filled { '█' } else { '·' };
                let a = if c < filled { ATTR_BRIGHT } else { ATTR_DIM };
                g.set(row, 23 + c, Cell::new(ch, a));
            }
            g.set(row, 23 + bar_w, Cell::new(']', ATTR_DIM));
            g.write(row, 23 + bar_w + 2, ATTR_NORMAL, &format_val(v));
        }
        draw_footer(g, "^v param   <> adjust   r reset   Esc back");
    }

    fn handle_key(&mut self, key: &str, ctx: &mut ScreenCtx) -> ScreenResult {
        let Some(pfx) = ctx.postfx.as_deref_mut() else {
            return ScreenResult::Pop;
        };
        let defs: Vec<crate::scene::ParamDef> = pfx
            .passes()
            .get(self.pass_idx)
            .map(|p| p.params.defs().to_vec())
            .unwrap_or_default();
        if defs.is_empty() {
            return match key {
                "Esc" | "Backspace" => ScreenResult::Pop,
                _ => ScreenResult::Continue,
            };
        }
        match key {
            "Esc" | "Backspace" => ScreenResult::Pop,
            "Up" => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                ScreenResult::Continue
            }
            "Down" => {
                if self.cursor + 1 < defs.len() {
                    self.cursor += 1;
                }
                ScreenResult::Continue
            }
            "Left" => {
                nudge(pfx, self.pass_idx, &defs[self.cursor], -1.0);
                let _ = pfx.save_state(ctx.state_dir);
                sync_active_look(ctx.looks.as_deref_mut(), ctx.active_look_slot, pfx);
                ScreenResult::Continue
            }
            "Right" => {
                nudge(pfx, self.pass_idx, &defs[self.cursor], 1.0);
                let _ = pfx.save_state(ctx.state_dir);
                sync_active_look(ctx.looks.as_deref_mut(), ctx.active_look_slot, pfx);
                ScreenResult::Continue
            }
            "r" | "R" => {
                if let Some(pm) = pfx.pass_params_mut(self.pass_idx) {
                    let d = &defs[self.cursor];
                    pm.set(&d.name, d.default);
                }
                let _ = pfx.save_state(ctx.state_dir);
                sync_active_look(ctx.looks.as_deref_mut(), ctx.active_look_slot, pfx);
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }
}

/// Write the live chain to the active+bound Look slot, if any. Called after
/// every postfx mutation (toggle, nudge, reset).
fn sync_active_look(
    looks: Option<&mut crate::preset::LookStore>,
    slot: Option<u8>,
    pfx: &dyn crate::render::postfx::PostFxController,
) {
    if let Some(looks) = looks {
        if let Err(e) = looks.after_postfx_mutation(slot, pfx.snapshot()) {
            tracing::warn!("postfx auto-sync: {e}");
        }
    }
}

fn nudge(
    pfx: &mut dyn crate::render::postfx::PostFxController,
    pass_idx: usize,
    d: &crate::scene::ParamDef,
    dir: f32,
) {
    if let Some(pm) = pfx.pass_params_mut(pass_idx) {
        let cur = pm.get(&d.name).unwrap_or(d.default);
        let step = (d.max - d.min) * 0.02;
        let next = (cur + dir * step).clamp(d.min, d.max);
        pm.set(&d.name, next);
    }
}

fn format_val(v: f32) -> String {
    if v.abs() >= 100.0 {
        format!("{v:>5.0}")
    } else if v.abs() >= 10.0 {
        format!("{v:>5.1}")
    } else {
        format!("{v:>5.2}")
    }
}

fn short_val(v: f32) -> String {
    if v.abs() >= 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    }
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
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
    use crate::audio::params::AudioParams;
    use crate::preset::SlotBindings;

    fn s_ctx<'a>(
        scenes: &'a [String],
        bindings: &'a mut SlotBindings,
        dir: &'a std::path::Path,
        audio: &'a std::sync::Arc<AudioParams>,
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
            looks: None,
        }
    }

    #[test]
    fn esc_pops() {
        let mut s = PostFxScreen::new();
        let scenes: Vec<String> = vec![];
        let mut b = SlotBindings::default();
        let dir = std::path::PathBuf::from("/tmp");
        let audio = AudioParams::new();
        let r = s.handle_key("Esc", &mut s_ctx(&scenes, &mut b, &dir, &audio));
        assert!(matches!(r, ScreenResult::Pop));
    }

    #[test]
    fn empty_chain_renders_placeholder() {
        let s = PostFxScreen::new();
        let mut g = TextScreen::new();
        let b = SlotBindings::default();
        let scenes: Vec<String> = vec![];
        let audio = AudioParams::new();
        let rctx = RenderCtx {
            scenes: &scenes,
            bindings: &b,
            audio: &audio,
            postfx: None,
            chromakey: None,
            filtered_scenes: 0,
            pi_gen: crate::platform::PiGen::Unknown,
            video_status: crate::video::VideoStatus::NoDevice,
            active_look_slot: None,
            bound_state: None,
            looks_view: None,
        };
        s.render(&mut g, &rctx);
        // The placeholder for "no postfx" starts at row 4 col 3.
        let row4: String = (3..20).map(|c| g.at(4, c).ch).collect();
        assert!(row4.starts_with("(post-fx"));
    }

    #[test]
    fn bind_first_press_captures_snapshot() {
        // We can't construct a real `&mut PostFx` without GL, so this test
        // exercises the *contract* of the bind path (the data flow the
        // handler performs) rather than dispatching through handle_key.
        use crate::preset::LookStore;
        use crate::scene::{LoadedScene, SceneLibrary, SceneMeta};
        use crate::state::{BlendMode, SharedState};
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.json");

        let mut lib = SceneLibrary::default();
        let meta_a = SceneMeta::parse(
            "name = \"a\"\n[[params]]\nslot = 0\nname = \"x\"\nmin = 0.0\nmax = 1.0\ndefault = 0.5\n",
            "x",
        ).unwrap();
        lib.upsert("a", LoadedScene {
            meta: meta_a,
            fragment_body: "void main() {}".into(),
            source_path: std::path::PathBuf::from("inline"),
            is_hq: false,
        });
        let meta_b = SceneMeta::parse(
            "name = \"b\"\n[[params]]\nslot = 0\nname = \"y\"\nmin = 0.0\nmax = 1.0\ndefault = 0.5\n",
            "x",
        ).unwrap();
        lib.upsert("b", LoadedScene {
            meta: meta_b,
            fragment_body: "void main() {}".into(),
            source_path: std::path::PathBuf::from("inline"),
            is_hq: false,
        });
        let state = SharedState::from_initial(&lib, "a", "b", 0.0, BlendMode::Mix).unwrap();
        let mut store = LookStore::load_or_empty(&path).unwrap();
        store.save(1, &state, None).unwrap();
        assert!(!store.has_snapshot(1));

        use crate::render::postfx;
        let passes = vec![postfx::tests_fake_pass("vignette", true, &[("amount", 0.7)])];
        let snap = postfx::snapshot_passes(&passes);
        store.save_postfx_snapshot(1, snap).unwrap();
        assert!(store.is_bound_active(1));
        let p = store.file.slots.get("1").unwrap();
        let saved = p.postfx.as_ref().unwrap();
        assert_eq!(saved.passes.len(), 1);
        assert_eq!(saved.passes[0].name, "vignette");
        assert!(saved.passes[0].enabled);
        assert_eq!(saved.passes[0].params.get("amount"), Some(&0.7));
    }

    #[test]
    fn param_screen_with_missing_pass_renders_placeholder() {
        let s = PostFxParamScreen::new(99);
        let mut g = TextScreen::new();
        let b = SlotBindings::default();
        let scenes: Vec<String> = vec![];
        let audio = AudioParams::new();
        let rctx = RenderCtx {
            scenes: &scenes,
            bindings: &b,
            audio: &audio,
            postfx: None,
            chromakey: None,
            filtered_scenes: 0,
            pi_gen: crate::platform::PiGen::Unknown,
            video_status: crate::video::VideoStatus::NoDevice,
            active_look_slot: None,
            bound_state: None,
            looks_view: None,
        };
        s.render(&mut g, &rctx);
        let row4: String = (3..30).map(|c| g.at(4, c).ch).collect();
        assert!(row4.contains("post-fx") || row4.contains("pass"));
    }
}
