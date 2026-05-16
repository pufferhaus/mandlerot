//! In-app menus rendered onto the status panel.
//!
//! Screens are a stack: while the stack is non-empty, key input is routed
//! to the top screen (instead of through the keymap). Each screen owns its
//! own local UI state (cursor, scroll). The currently-visible grid is
//! rendered on the main thread and shipped to the status thread inside the
//! per-frame snapshot — so the main thread is the only place that holds
//! `Box<dyn Screen>`.

use std::path::Path;
use std::sync::Arc;

use crate::audio::params::AudioParams;
use crate::preset::SlotBindings;
use crate::render::postfx::PostFx;
use crate::status::TextScreen;

pub mod screens;

/// Context passed to a screen each time it handles a key. Holds the
/// mutable bits a screen may need to act on (write a binding, look up
/// scene names) so the screen itself doesn't need to capture them.
pub struct ScreenCtx<'a> {
    pub scenes: &'a [String],
    pub bindings: &'a mut SlotBindings,
    pub state_dir: &'a Path,
    pub audio: &'a Arc<AudioParams>,
    /// Live post-FX chain. Production main loop always sets this. Tests
    /// that don't exercise post-FX screens pass `None` to avoid having to
    /// build a GL context just to construct a `PostFx`.
    pub postfx: Option<&'a mut PostFx>,
    /// Live video-capture status, surfaced so screens that want to label
    /// "no device" / "OK" / "stale" / "error" can do so without reaching
    /// back into the pipeline.
    pub video_status: crate::video::VideoStatus,
}

/// Read-only context for paint time. Decoupled from `ScreenCtx` so the
/// render path can run after handle_key (and so render doesn't need a
/// mutable borrow on bindings).
pub struct RenderCtx<'a> {
    pub scenes: &'a [String],
    pub bindings: &'a SlotBindings,
    pub audio: &'a Arc<AudioParams>,
    pub postfx: Option<&'a PostFx>,
    /// Number of scenes dropped at load time because their `min_pi_gen`
    /// exceeded the detected gen (roadmap 28a). 0 = no filtering applied.
    pub filtered_scenes: usize,
    /// Detected Pi generation, surfaced for screens that want to label
    /// "unsupported on <gen>".
    pub pi_gen: crate::platform::PiGen,
    /// Live video-capture status, surfaced for screens that want to render
    /// the current device state (Task 13's audio/video device picker).
    pub video_status: crate::video::VideoStatus,
}

/// Result of a single key delivered to a screen.
pub enum ScreenResult {
    /// Stay open, render again on the next frame.
    Continue,
    /// Close this screen (pop one level). If the stack becomes empty, the
    /// status panel returns to its normal status compose view.
    Pop,
    /// Push a new screen on top. The old screen is preserved underneath and
    /// will become active again once the new one Pops.
    Push(Box<dyn Screen>),
}

pub trait Screen: Send {
    /// Paint the entire 80×26 grid for this screen.
    fn render(&self, g: &mut TextScreen, ctx: &RenderCtx);
    /// Receive a key press (e.g. `"1"`, `"Up"`, `"Enter"`, `"Esc"`).
    fn handle_key(&mut self, key: &str, ctx: &mut ScreenCtx) -> ScreenResult;
}

/// Translate rotated-numpad keys into arrow strings before the screen
/// stack sees them. The cheap production numpad is mounted rotated 90°
/// CCW; from the operator's POV the centre cross of the 3×3 digit block
/// is the natural arrow cluster:
///
/// ```text
///   .  Up  .          physical:   .  6  .
///  Lt  .  Rt                       8  .  2
///   . Dn  .                        .  4  .
/// ```
///
/// Only the four centre-cross keys are translated. Everything else
/// (corners, digits 0/5, operators, non-numpad keys) passes through
/// untouched.
fn translate_numpad(key: &str) -> &str {
    match key {
        "Numpad6" => "Up",
        "Numpad4" => "Down",
        "Numpad8" => "Left",
        "Numpad2" => "Right",
        other => other,
    }
}

#[derive(Default)]
pub struct ScreenStack {
    stack: Vec<Box<dyn Screen>>,
}

impl ScreenStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        !self.stack.is_empty()
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    pub fn open(&mut self, screen: Box<dyn Screen>) {
        self.stack.push(screen);
    }

    /// Close all screens (e.g. when the user mashes a global hotkey).
    pub fn close_all(&mut self) {
        self.stack.clear();
    }

    /// Deliver a key to the top screen and apply the resulting action.
    /// Returns `true` if the key was consumed (the menu was open).
    pub fn handle_key(&mut self, key: &str, ctx: &mut ScreenCtx) -> bool {
        if self.stack.is_empty() {
            return false;
        }
        let Some(top) = self.stack.last_mut() else {
            return false;
        };
        let key = translate_numpad(key);
        let result = top.handle_key(key, ctx);
        match result {
            ScreenResult::Continue => {}
            ScreenResult::Pop => {
                self.stack.pop();
            }
            ScreenResult::Push(s) => {
                self.stack.push(s);
            }
        }
        true
    }

    /// Render the top screen into a fresh `TextScreen`. `None` if no screen
    /// is open — callers fall back to the regular status compose.
    pub fn render_top(&self, ctx: &RenderCtx) -> Option<TextScreen> {
        let top = self.stack.last()?;
        let mut g = TextScreen::new();
        top.render(&mut g, ctx);
        Some(g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dummy {
        label: char,
        popped_after: u8,
        pressed: u8,
    }

    impl Screen for Dummy {
        fn render(&self, g: &mut TextScreen, _ctx: &RenderCtx) {
            g.write(0, 0, crate::status::ATTR_NORMAL, &self.label.to_string());
        }
        fn handle_key(&mut self, _key: &str, _ctx: &mut ScreenCtx) -> ScreenResult {
            self.pressed += 1;
            if self.pressed >= self.popped_after {
                ScreenResult::Pop
            } else {
                ScreenResult::Continue
            }
        }
    }

    fn empty_ctx<'a>(
        scenes: &'a [String],
        bindings: &'a mut SlotBindings,
        dir: &'a Path,
        audio: &'a Arc<AudioParams>,
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

    fn render_ctx<'a>(
        scenes: &'a [String],
        bindings: &'a SlotBindings,
        audio: &'a Arc<AudioParams>,
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
    fn empty_stack_returns_no_top() {
        let s = ScreenStack::new();
        assert!(!s.is_open());
        let b = SlotBindings::default();
        let scenes: Vec<String> = vec![];
        let audio = AudioParams::new();
        assert!(s.render_top(&render_ctx(&scenes, &b, &audio)).is_none());
    }

    #[test]
    fn push_then_pop_closes() {
        let mut s = ScreenStack::new();
        s.open(Box::new(Dummy {
            label: 'A',
            popped_after: 1,
            pressed: 0,
        }));
        assert!(s.is_open());
        let mut binds = SlotBindings::default();
        let scenes: Vec<String> = vec![];
        let dir = std::path::PathBuf::from("/tmp");
        let audio = AudioParams::new();
        let mut ctx = empty_ctx(&scenes, &mut binds, &dir, &audio);
        let consumed = s.handle_key("X", &mut ctx);
        assert!(consumed);
        assert!(!s.is_open());
    }

    #[test]
    fn pushed_screen_becomes_top() {
        struct Parent;
        impl Screen for Parent {
            fn render(&self, g: &mut TextScreen, _c: &RenderCtx) {
                g.write(0, 0, crate::status::ATTR_NORMAL, "P");
            }
            fn handle_key(&mut self, _k: &str, _c: &mut ScreenCtx) -> ScreenResult {
                ScreenResult::Push(Box::new(Dummy {
                    label: 'C',
                    popped_after: 99,
                    pressed: 0,
                }))
            }
        }
        let mut s = ScreenStack::new();
        s.open(Box::new(Parent));
        let mut binds = SlotBindings::default();
        let scenes: Vec<String> = vec![];
        let dir = std::path::PathBuf::from("/tmp");
        let audio = AudioParams::new();
        let mut ctx = empty_ctx(&scenes, &mut binds, &dir, &audio);
        s.handle_key("any", &mut ctx);
        assert_eq!(s.depth(), 2);
        let grid = s
            .render_top(&render_ctx(&scenes, &binds, &audio))
            .unwrap();
        // Top of stack ('C') should be what's rendered.
        assert_eq!(grid.at(0, 0).ch, 'C');
    }

    #[test]
    fn unhandled_when_empty() {
        let mut s = ScreenStack::new();
        let mut binds = SlotBindings::default();
        let scenes: Vec<String> = vec![];
        let dir = std::path::PathBuf::from("/tmp");
        let audio = AudioParams::new();
        let mut ctx = empty_ctx(&scenes, &mut binds, &dir, &audio);
        let consumed = s.handle_key("Esc", &mut ctx);
        assert!(!consumed);
    }

    #[test]
    fn numpad_translates_to_arrows() {
        assert_eq!(translate_numpad("Numpad6"), "Up");
        assert_eq!(translate_numpad("Numpad4"), "Down");
        assert_eq!(translate_numpad("Numpad8"), "Left");
        assert_eq!(translate_numpad("Numpad2"), "Right");
    }

    #[test]
    fn translate_numpad_passes_through_everything_else() {
        for k in [
            "Numpad0", "Numpad1", "Numpad3", "Numpad5", "Numpad7", "Numpad9",
            "NumpadEnter", "NumpadAdd", "NumpadSubtract", "NumpadDecimal",
            "NumpadMultiply", "NumpadDivide", "Up", "Down", "Left", "Right",
            "Enter", "Esc", "Backspace", "1", "6", "a",
        ] {
            assert_eq!(translate_numpad(k), k, "expected {k} to pass through");
        }
    }

    #[test]
    fn stack_translates_numpad6_to_up_for_top_screen() {
        use std::sync::Mutex;
        static LAST: Mutex<String> = Mutex::new(String::new());

        struct Recorder;
        impl Screen for Recorder {
            fn render(&self, _g: &mut TextScreen, _c: &RenderCtx) {}
            fn handle_key(&mut self, key: &str, _c: &mut ScreenCtx) -> ScreenResult {
                *LAST.lock().unwrap() = key.to_string();
                ScreenResult::Continue
            }
        }

        *LAST.lock().unwrap() = String::new();
        let mut s = ScreenStack::new();
        s.open(Box::new(Recorder));

        let mut binds = SlotBindings::default();
        let scenes: Vec<String> = vec![];
        let dir = std::path::PathBuf::from("/tmp");
        let audio = AudioParams::new();
        let mut ctx = empty_ctx(&scenes, &mut binds, &dir, &audio);

        assert!(s.handle_key("Numpad6", &mut ctx));
        assert_eq!(*LAST.lock().unwrap(), "Up");
        assert!(s.handle_key("Numpad2", &mut ctx));
        assert_eq!(*LAST.lock().unwrap(), "Right");
        assert!(s.handle_key("Numpad5", &mut ctx));
        assert_eq!(*LAST.lock().unwrap(), "Numpad5");
    }
}
