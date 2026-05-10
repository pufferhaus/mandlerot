//! Offscreen framebuffer for per-layer scene rendering.

use std::sync::Arc;

use glow::HasContext;

use crate::error::{Error, Result};

pub struct Fbo {
    gl: Arc<glow::Context>,
    pub framebuffer: glow::Framebuffer,
    pub texture: glow::Texture,
    pub width: u32,
    pub height: u32,
}

impl Fbo {
    pub fn new(gl: Arc<glow::Context>, width: u32, height: u32) -> Result<Self> {
        unsafe {
            let texture = gl
                .create_texture()
                .map_err(|e| Error::Backend(format!("create texture: {e}")))?;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                width as i32,
                height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                None,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );

            let framebuffer = gl
                .create_framebuffer()
                .map_err(|e| Error::Backend(format!("create fbo: {e}")))?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture),
                0,
            );
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            if status != glow::FRAMEBUFFER_COMPLETE {
                return Err(Error::Backend(format!("FBO incomplete: {status:#x}")));
            }
            // Clear to opaque black so the very first `u_prev` sample on a
            // ping-pong setup reads (0,0,0,1) rather than uninitialized data.
            gl.viewport(0, 0, width as i32, height as i32);
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);

            Ok(Self {
                gl,
                framebuffer,
                texture,
                width,
                height,
            })
        }
    }

    pub fn bind(&self) {
        unsafe {
            self.gl
                .bind_framebuffer(glow::FRAMEBUFFER, Some(self.framebuffer));
            self.gl
                .viewport(0, 0, self.width as i32, self.height as i32);
        }
    }
}

impl Drop for Fbo {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_framebuffer(self.framebuffer);
            self.gl.delete_texture(self.texture);
        }
    }
}
