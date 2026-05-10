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
    /// Ping-pong FBOs for layer A. Frame N writes to `fbo_a[front_a]`
    /// while sampling `fbo_a[1 - front_a]` as `u_prev`.
    fbo_a: [Fbo; 2],
    fbo_b: [Fbo; 2],
    front_a: usize,
    front_b: usize,
    blend_program: glow::Program,
    quad_vao: glow::VertexArray,
    /// scene name → compiled program
    scene_programs: BTreeMap<String, glow::Program>,
    overlay_program: Option<glow::Program>,
}

impl Pipeline {
    pub fn new(gl: Arc<glow::Context>, width: u32, height: u32) -> Result<Self> {
        let fbo_a = [
            Fbo::new(gl.clone(), width, height)?,
            Fbo::new(gl.clone(), width, height)?,
        ];
        let fbo_b = [
            Fbo::new(gl.clone(), width, height)?,
            Fbo::new(gl.clone(), width, height)?,
        ];
        let blend_program = compile_program(&gl, QUAD_VERT, BLEND_FRAG)?;
        let quad_vao = create_quad_vao(&gl)?;
        Ok(Self {
            gl,
            width,
            height,
            fbo_a,
            fbo_b,
            front_a: 0,
            front_b: 0,
            blend_program,
            quad_vao,
            scene_programs: BTreeMap::new(),
            overlay_program: None,
        })
    }

    /// Index of the FBO holding the most recently rendered frame for layer A.
    pub fn front_a(&self) -> usize {
        self.front_a
    }
    /// Index of the FBO holding the most recently rendered frame for layer B.
    pub fn front_b(&self) -> usize {
        self.front_b
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
        prev: &Fbo,
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
            // Bind the previous-frame texture for this layer to TEXTURE0
            // so the scene shader can sample its own history via `u_prev`.
            // No other texture is in flight during a scene render, so unit 0
            // is free; the blend pass re-binds units 0/1 fresh.
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(prev.texture));
            set_uniform_int(&self.gl, *prog, "u_prev", 0);
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
            let bands = if state.audio_bypass {
                [0.0; 4]
            } else {
                state.audio_bands
            };
            set_uniform_vec4(
                &self.gl, *prog, "u_audio", bands[0], bands[1], bands[2], bands[3],
            );
            set_uniform_float(&self.gl, *prog, "u_trigger", state.trigger);
            // Beat trigger is the same value as `u_trigger` for now; scenes
            // can prefer one or the other. Zero when bypassed.
            let beat_uniform = if state.audio_bypass {
                0.0
            } else {
                state.trigger
            };
            set_uniform_float(&self.gl, *prog, "u_beat", beat_uniform);
            set_uniform_float(&self.gl, *prog, "u_bpm", state.tap_tempo_bpm);
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
                .bind_texture(glow::TEXTURE_2D, Some(self.fbo_a[self.front_a].texture));
            set_uniform_int(&self.gl, self.blend_program, "u_layer_a", 0);
            self.gl.active_texture(glow::TEXTURE1);
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.fbo_b[self.front_b].texture));
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

    pub fn frame(
        &mut self,
        state: &SharedState,
        default_fb_w: u32,
        default_fb_h: u32,
    ) -> Result<()> {
        // Layer A: write to the back FBO, sample the front (previous frame).
        let next_a = 1 - self.front_a;
        self.render_layer_to(
            &self.fbo_a[next_a],
            &self.fbo_a[self.front_a],
            &state.layer_a.scene_name,
            &state.layer_a.params,
            state,
        )?;
        self.front_a = next_a;

        let next_b = 1 - self.front_b;
        self.render_layer_to(
            &self.fbo_b[next_b],
            &self.fbo_b[self.front_b],
            &state.layer_b.scene_name,
            &state.layer_b.params,
            state,
        )?;
        self.front_b = next_b;

        self.render_blend_to_default(default_fb_w, default_fb_h, state);
        Ok(())
    }

    /// Upload an RGBA strip and draw it at `(origin_x, origin_y)` on the
    /// currently bound framebuffer. Caller is responsible for binding the right
    /// target first.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_overlay_strip(
        &mut self,
        rgba: &[u8],
        strip_w: u32,
        strip_h: u32,
        origin_x: u32,
        origin_y: u32,
        target_w: u32,
        target_h: u32,
    ) {
        unsafe {
            // Lazily create the overlay program on first use.
            if self.overlay_program.is_none() {
                let prog = compile_program(&self.gl, OVERLAY_VERT, OVERLAY_FRAG).ok();
                self.overlay_program = prog;
            }
            let Some(prog) = self.overlay_program else {
                return;
            };
            let tex = match self.gl.create_texture() {
                Ok(t) => t,
                Err(_) => return,
            };
            self.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            self.gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                strip_w as i32,
                strip_h as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                Some(rgba),
            );
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );

            self.gl.use_program(Some(prog));
            self.gl.active_texture(glow::TEXTURE0);
            set_uniform_int(&self.gl, prog, "u_overlay_tex", 0);
            set_uniform_vec2(
                &self.gl,
                prog,
                "u_resolution",
                target_w as f32,
                target_h as f32,
            );
            set_uniform_vec2(
                &self.gl,
                prog,
                "u_overlay_size",
                strip_w as f32,
                strip_h as f32,
            );
            set_uniform_vec2(
                &self.gl,
                prog,
                "u_overlay_origin",
                origin_x as f32,
                origin_y as f32,
            );
            self.gl.enable(glow::BLEND);
            self.gl
                .blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            self.gl.bind_vertex_array(Some(self.quad_vao));
            self.gl.draw_arrays(glow::TRIANGLES, 0, VERTEX_COUNT);
            self.gl.disable(glow::BLEND);
            self.gl.delete_texture(tex);
        }
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

const OVERLAY_VERT: &str = include_str!("../../shaders/quad.vert");
const OVERLAY_FRAG: &str = include_str!("../../shaders/overlay.glsl");

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

    fn make_state(lib: &crate::scene::SceneLibrary, scene: &str) -> crate::state::SharedState {
        crate::state::SharedState::from_initial(
            lib,
            scene,
            scene,
            0.0,
            crate::state::BlendMode::Mix,
        )
        .unwrap()
    }

    fn lib_with(scene_name: &str, body: &str) -> crate::scene::SceneLibrary {
        let mut lib = crate::scene::SceneLibrary::default();
        let scene = loaded(scene_name, body);
        lib.upsert(scene_name, scene);
        lib
    }

    #[test]
    #[ignore = "Requires display, main thread"]
    fn pingpong_constructs_two_fbo_pairs() {
        let target = WinitGlTarget::new(8, 8, "test").unwrap();
        let gl: Arc<glow::Context> = target.gl();
        let pipe = Pipeline::new(gl, 8, 8).unwrap();
        // Each layer has exactly two FBOs (the array literal guarantees this
        // structurally; this assertion exists to catch regressions if the
        // shape ever changes back to a single FBO).
        assert_eq!(pipe.fbo_a.len(), 2);
        assert_eq!(pipe.fbo_b.len(), 2);
        assert_eq!(pipe.front_a(), 0);
        assert_eq!(pipe.front_b(), 0);
    }

    #[test]
    #[ignore = "Requires display, main thread"]
    fn front_index_flips_each_frame() {
        let target = WinitGlTarget::new(8, 8, "test").unwrap();
        let gl: Arc<glow::Context> = target.gl();
        let mut pipe = Pipeline::new(gl, 8, 8).unwrap();
        let lib = lib_with("solid", "void main() { gl_FragColor = vec4(1.0); }");
        pipe.upsert_scene("solid", lib.get("solid").unwrap())
            .unwrap();
        let state = make_state(&lib, "solid");
        assert_eq!(pipe.front_a(), 0);
        assert_eq!(pipe.front_b(), 0);
        pipe.frame(&state, 8, 8).unwrap();
        assert_eq!(pipe.front_a(), 1);
        assert_eq!(pipe.front_b(), 1);
        pipe.frame(&state, 8, 8).unwrap();
        assert_eq!(pipe.front_a(), 0);
        assert_eq!(pipe.front_b(), 0);
    }

    #[test]
    #[ignore = "Requires display, main thread"]
    fn fbos_clear_to_opaque_black_on_construct() {
        // First-frame `u_prev` should sample (0,0,0,1). We verify by running a
        // scene that simply outputs `texture2D(u_prev, v_uv)` on frame 0;
        // since neither FBO has been written, the entire output should be
        // black (alpha=1).
        let target = WinitGlTarget::new(8, 8, "test").unwrap();
        let gl: Arc<glow::Context> = target.gl();
        let mut pipe = Pipeline::new(gl, 8, 8).unwrap();
        let lib = lib_with(
            "echo_test",
            "void main() { gl_FragColor = texture2D(u_prev, v_uv); }",
        );
        pipe.upsert_scene("echo_test", lib.get("echo_test").unwrap())
            .unwrap();
        let state = make_state(&lib, "echo_test");
        pipe.frame(&state, 8, 8).unwrap();
        let pixels = pipe.read_default_pixels(8, 8);
        // Every channel of every pixel should be 0 (RGB) with alpha 255.
        for px in pixels.chunks_exact(4) {
            assert_eq!(px[0], 0, "R should be 0 on first frame");
            assert_eq!(px[1], 0, "G should be 0 on first frame");
            assert_eq!(px[2], 0, "B should be 0 on first frame");
        }
    }

    #[test]
    #[ignore = "Requires display, main thread"]
    fn u_prev_accumulates_across_frames() {
        // Scene adds 0.1 to its previous frame each tick. After N frames we
        // should see roughly N*0.1 (capped at 1.0). This is the canonical
        // ping-pong correctness proof.
        let target = WinitGlTarget::new(8, 8, "test").unwrap();
        let gl: Arc<glow::Context> = target.gl();
        let mut pipe = Pipeline::new(gl, 8, 8).unwrap();
        let lib = lib_with(
            "accum",
            "void main() { gl_FragColor = texture2D(u_prev, v_uv) + vec4(0.1, 0.1, 0.1, 0.0); }",
        );
        pipe.upsert_scene("accum", lib.get("accum").unwrap())
            .unwrap();
        let state = make_state(&lib, "accum");
        // Run 5 frames → expected ~0.5 in each channel (≈127).
        for _ in 0..5 {
            pipe.frame(&state, 8, 8).unwrap();
        }
        let pixels = pipe.read_default_pixels(8, 8);
        // Allow generous tolerance: GPU low-precision blend + 8-bit roundoff.
        let center = pixels[(4 * 8 + 4) * 4] as i32;
        assert!(
            (100..=160).contains(&center),
            "expected ~127 after 5 accumulations, got {center}"
        );
    }

    #[test]
    #[ignore = "Requires display, main thread"]
    fn u_prev_per_layer_isolation() {
        // Layer A's `u_prev` must reflect layer A history only — not layer B.
        // Run scene A = "constant red", scene B = "constant blue", verify
        // each layer's previous-frame texture stays its own color.
        let target = WinitGlTarget::new(8, 8, "test").unwrap();
        let gl: Arc<glow::Context> = target.gl();
        let mut pipe = Pipeline::new(gl, 8, 8).unwrap();
        let mut lib = crate::scene::SceneLibrary::default();
        lib.upsert(
            "red",
            loaded(
                "red",
                "void main() { gl_FragColor = vec4(1.0, 0.0, 0.0, 1.0); }",
            ),
        );
        lib.upsert(
            "blue",
            loaded(
                "blue",
                "void main() { gl_FragColor = vec4(0.0, 0.0, 1.0, 1.0); }",
            ),
        );
        pipe.upsert_scene("red", lib.get("red").unwrap()).unwrap();
        pipe.upsert_scene("blue", lib.get("blue").unwrap()).unwrap();
        let mut state = crate::state::SharedState::from_initial(
            &lib,
            "red",
            "blue",
            0.5,
            crate::state::BlendMode::Mix,
        )
        .unwrap();
        state.xfade = 0.5; // see both
        pipe.frame(&state, 8, 8).unwrap();
        pipe.frame(&state, 8, 8).unwrap();
        // After two frames, blended output should have non-trivial red AND
        // blue. If isolation were broken we'd see purple bleeding asymmetrically.
        let pixels = pipe.read_default_pixels(8, 8);
        let px = &pixels[0..4];
        assert!(px[0] > 64, "red should survive in layer A: {px:?}");
        assert!(px[2] > 64, "blue should survive in layer B: {px:?}");
    }
}
