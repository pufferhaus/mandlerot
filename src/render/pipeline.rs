//! Per-frame GL orchestration. Compiles scene programs, renders A/B layers
//! to FBOs, then blends to the default framebuffer.

use std::collections::BTreeMap;
use std::sync::Arc;

use glow::HasContext;

use crate::error::{Error, Result};
use crate::scene::{LoadedScene, ParamMap};
use crate::state::SharedState;

use super::fbo::Fbo;
use super::quad::{QUAD_POSITIONS, VERTEX_COUNT};
use super::shader::{assemble_scene_fragment, BLEND_FRAG, QUAD_VERT};

pub struct Pipeline {
    gl: Arc<glow::Context>,
    width: u32,
    height: u32,
    fbo_a: Fbo,
    fbo_b: Fbo,
    blend_program: glow::Program,
    quad_vao: glow::VertexArray,
    /// scene name → compiled program
    scene_programs: BTreeMap<String, glow::Program>,
}

impl Pipeline {
    pub fn new(gl: Arc<glow::Context>, width: u32, height: u32) -> Result<Self> {
        let fbo_a = Fbo::new(gl.clone(), width, height)?;
        let fbo_b = Fbo::new(gl.clone(), width, height)?;
        let blend_program = compile_program(&gl, QUAD_VERT, BLEND_FRAG)?;
        let quad_vao = create_quad_vao(&gl)?;
        Ok(Self {
            gl,
            width,
            height,
            fbo_a,
            fbo_b,
            blend_program,
            quad_vao,
            scene_programs: BTreeMap::new(),
        })
    }

    /// Compile (or recompile) a scene's program. On failure, returns the GL
    /// info-log without modifying registered programs.
    pub fn upsert_scene(&mut self, name: &str, scene: &LoadedScene) -> Result<()> {
        let frag = assemble_scene_fragment(&scene.fragment_body);
        let new_prog = compile_program(&self.gl, QUAD_VERT, &frag)?;
        if let Some(old) = self.scene_programs.insert(name.to_string(), new_prog) {
            unsafe { self.gl.delete_program(old) };
        }
        Ok(())
    }

    pub fn has_scene(&self, name: &str) -> bool {
        self.scene_programs.contains_key(name)
    }

    fn render_layer_to(
        &self,
        target: &Fbo,
        scene_name: &str,
        params: &ParamMap,
        state: &SharedState,
    ) -> Result<()> {
        let prog = self
            .scene_programs
            .get(scene_name)
            .ok_or_else(|| Error::SceneNotFound(scene_name.into()))?;
        target.bind();
        unsafe {
            self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
            self.gl.use_program(Some(*prog));
            set_uniform_float(&self.gl, *prog, "u_time", state.time_secs);
            set_uniform_vec2(
                &self.gl,
                *prog,
                "u_resolution",
                self.width as f32,
                self.height as f32,
            );
            // Zero out audio bands when audio is bypassed so scenes can't
            // see leaked-through reactivity through the u_audio uniform.
            // Per-slot routing is already gated by `audio_bypass` inside
            // `effective_slot_values`, but `u_audio` was direct.
            let bands = if state.audio_bypass { [0.0; 4] } else { state.audio_bands };
            set_uniform_vec4(
                &self.gl,
                *prog,
                "u_audio",
                bands[0],
                bands[1],
                bands[2],
                bands[3],
            );
            set_uniform_float(&self.gl, *prog, "u_trigger", state.trigger);
            let slots = params.effective_slot_values(&state.audio_bands, state.audio_bypass);
            for (i, v) in slots.iter().enumerate() {
                set_uniform_float(&self.gl, *prog, &format!("u_param{i}"), *v);
            }
            self.gl.bind_vertex_array(Some(self.quad_vao));
            self.gl.draw_arrays(glow::TRIANGLES, 0, VERTEX_COUNT);
        }
        Ok(())
    }

    /// Bind whatever the caller wants as the final destination, then call.
    pub fn render_blend_to_default(
        &self,
        default_fb_w: u32,
        default_fb_h: u32,
        state: &SharedState,
    ) {
        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            self.gl
                .viewport(0, 0, default_fb_w as i32, default_fb_h as i32);
            self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
            self.gl.use_program(Some(self.blend_program));
            self.gl.active_texture(glow::TEXTURE0);
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.fbo_a.texture));
            set_uniform_int(&self.gl, self.blend_program, "u_layer_a", 0);
            self.gl.active_texture(glow::TEXTURE1);
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.fbo_b.texture));
            set_uniform_int(&self.gl, self.blend_program, "u_layer_b", 1);
            set_uniform_float(&self.gl, self.blend_program, "u_xfade", state.xfade);
            set_uniform_int(
                &self.gl,
                self.blend_program,
                "u_blend_mode",
                state.blend_mode.as_int(),
            );
            self.gl.bind_vertex_array(Some(self.quad_vao));
            self.gl.draw_arrays(glow::TRIANGLES, 0, VERTEX_COUNT);
        }
    }

    pub fn frame(&self, state: &SharedState, default_fb_w: u32, default_fb_h: u32) -> Result<()> {
        self.render_layer_to(
            &self.fbo_a,
            &state.layer_a.scene_name,
            &state.layer_a.params,
            state,
        )?;
        self.render_layer_to(
            &self.fbo_b,
            &state.layer_b.scene_name,
            &state.layer_b.params,
            state,
        )?;
        self.render_blend_to_default(default_fb_w, default_fb_h, state);
        Ok(())
    }

    /// Read default framebuffer back into RGBA8 vec (for headless tests).
    pub fn read_default_pixels(&self, w: u32, h: u32) -> Vec<u8> {
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            self.gl.read_pixels(
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(&mut pixels),
            );
        }
        pixels
    }
}

fn compile_program(gl: &glow::Context, vert: &str, frag: &str) -> Result<glow::Program> {
    unsafe {
        let v = compile_shader(gl, glow::VERTEX_SHADER, vert)?;
        let f = compile_shader(gl, glow::FRAGMENT_SHADER, frag)?;
        let prog = gl
            .create_program()
            .map_err(|e| Error::Backend(format!("create program: {e}")))?;
        gl.attach_shader(prog, v);
        gl.attach_shader(prog, f);
        gl.bind_attrib_location(prog, 0, "a_pos");
        gl.link_program(prog);
        if !gl.get_program_link_status(prog) {
            let log = gl.get_program_info_log(prog);
            gl.delete_program(prog);
            gl.delete_shader(v);
            gl.delete_shader(f);
            return Err(Error::ShaderCompile(log));
        }
        gl.detach_shader(prog, v);
        gl.detach_shader(prog, f);
        gl.delete_shader(v);
        gl.delete_shader(f);
        Ok(prog)
    }
}

unsafe fn compile_shader(gl: &glow::Context, kind: u32, src: &str) -> Result<glow::Shader> {
    let s = gl
        .create_shader(kind)
        .map_err(|e| Error::Backend(format!("create shader: {e}")))?;
    gl.shader_source(s, src);
    gl.compile_shader(s);
    if !gl.get_shader_compile_status(s) {
        let log = gl.get_shader_info_log(s);
        gl.delete_shader(s);
        return Err(Error::ShaderCompile(log));
    }
    Ok(s)
}

fn create_quad_vao(gl: &glow::Context) -> Result<glow::VertexArray> {
    unsafe {
        let vao = gl
            .create_vertex_array()
            .map_err(|e| Error::Backend(format!("create vao: {e}")))?;
        gl.bind_vertex_array(Some(vao));
        let vbo = gl
            .create_buffer()
            .map_err(|e| Error::Backend(format!("create vbo: {e}")))?;
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        let bytes: &[u8] = bytemuck::cast_slice(QUAD_POSITIONS);
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STATIC_DRAW);
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 0, 0);
        Ok(vao)
    }
}

unsafe fn set_uniform_float(gl: &glow::Context, prog: glow::Program, name: &str, v: f32) {
    let loc = gl.get_uniform_location(prog, name);
    gl.uniform_1_f32(loc.as_ref(), v);
}
unsafe fn set_uniform_int(gl: &glow::Context, prog: glow::Program, name: &str, v: i32) {
    let loc = gl.get_uniform_location(prog, name);
    gl.uniform_1_i32(loc.as_ref(), v);
}
unsafe fn set_uniform_vec2(gl: &glow::Context, prog: glow::Program, name: &str, x: f32, y: f32) {
    let loc = gl.get_uniform_location(prog, name);
    gl.uniform_2_f32(loc.as_ref(), x, y);
}
unsafe fn set_uniform_vec4(
    gl: &glow::Context,
    prog: glow::Program,
    name: &str,
    x: f32,
    y: f32,
    z: f32,
    w: f32,
) {
    let loc = gl.get_uniform_location(prog, name);
    gl.uniform_4_f32(loc.as_ref(), x, y, z, w);
}

#[cfg(test)]
#[cfg(feature = "desktop")]
mod tests {
    use super::*;
    use crate::render::desktop::WinitGlTarget;
    use crate::render::target::RenderTarget;
    use crate::scene::{LoadedScene, SceneMeta};
    use std::sync::Arc;

    fn loaded(name: &str, body: &str) -> LoadedScene {
        let meta_str = format!(
            "name = \"{}\"
[[params]]
slot = 0
name = \"x\"
min = 0.0
max = 1.0
default = 0.0
",
            name
        );
        LoadedScene {
            meta: SceneMeta::parse(&meta_str, "inline").unwrap(),
            fragment_body: body.to_string(),
            source_path: std::path::PathBuf::from("inline"),
        }
    }

    #[test]
    #[ignore = "Requires display, main thread"]
    fn bad_recompile_keeps_old_program() {
        let target = WinitGlTarget::new(64, 64, "test").unwrap();
        let gl: Arc<glow::Context> = target.gl();
        let mut pipe = Pipeline::new(gl, 64, 64).unwrap();
        let good = loaded("foo", "void main() { gl_FragColor = vec4(1.0); }");
        pipe.upsert_scene("foo", &good).unwrap();
        assert!(pipe.has_scene("foo"));

        let bad = loaded("foo", "this isn't glsl");
        let err = pipe.upsert_scene("foo", &bad).unwrap_err();
        assert!(matches!(err, crate::Error::ShaderCompile(_)));
        // old program still present
        assert!(pipe.has_scene("foo"));
    }
}
