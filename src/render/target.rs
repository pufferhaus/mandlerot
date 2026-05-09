//! Backend-agnostic render target.
//!
//! A target owns the GL context, exposes a `glow::Context` for drawing, and
//! presents the framebuffer (window swap or KMS page-flip).

use std::sync::Arc;

use glow::HasContext;

use crate::error::Result;

pub trait RenderTarget {
    /// Returns the live `glow::Context` for issuing draw calls.
    fn gl(&self) -> Arc<glow::Context>;

    /// Current backbuffer dimensions in pixels.
    fn dimensions(&self) -> (u32, u32);

    /// Present the rendered frame (swap or page-flip).
    fn present(&mut self) -> Result<()>;

    /// Pump platform events; return false if the user requested exit.
    fn pump(&mut self) -> bool;
}

/// Helper used by both backends after context creation: assert basic GLES2 capability.
pub fn assert_gles2_capable(gl: &glow::Context) -> Result<()> {
    let version = unsafe { gl.get_parameter_string(glow::VERSION) };
    tracing::info!(gl_version = %version, "GL context");
    let renderer = unsafe { gl.get_parameter_string(glow::RENDERER) };
    tracing::info!(gl_renderer = %renderer, "GL renderer");
    // GLES2 is a minimum; both desktop GL 2.1+ and GLES2 satisfy our shader set.
    if version.is_empty() {
        return Err(crate::Error::Backend("empty GL_VERSION".into()));
    }
    Ok(())
}
