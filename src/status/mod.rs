pub mod compose;
pub mod glyphs;
pub mod grid;
pub mod render;
pub mod theme;

#[cfg(feature = "desktop")]
pub mod desktop;

#[cfg(all(feature = "pi", target_os = "linux"))]
pub mod pi;

pub use grid::{Cell, TextScreen, ATTR_BRIGHT, ATTR_DIM, ATTR_INVERSE, ATTR_NORMAL, COLS, ROWS};

/// Output sink for the rendered framebuffer.
pub trait Backend: Send {
    /// Push the entire framebuffer.
    fn flush_full(&mut self, fb: &render::Fb) -> crate::Result<()>;
    /// Push only the given run rectangles. Implementations may collapse to
    /// `flush_full` if partial updates aren't beneficial.
    fn flush_runs(&mut self, fb: &render::Fb, runs: &[(usize, usize, usize)]) -> crate::Result<()>;
}
