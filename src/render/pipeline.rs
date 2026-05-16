//! Per-frame GL orchestration. Compiles scene programs, renders A/B layers
//! to FBOs, then blends to the default framebuffer.

use std::collections::BTreeMap;
use std::sync::Arc;

use glow::HasContext;

use crate::error::{Error, Result};
use crate::platform::PiGen;
use crate::scene::{LoadedScene, ParamMap};
use crate::state::SharedState;

use super::fbo::Fbo;
use super::postfx::PostFx;
use super::quad::{QUAD_POSITIONS, VERTEX_COUNT};
use super::shader::{assemble_scene_fragment, BLEND_FRAG, QUAD_VERT};

/// Cached uniform locations for one scene program. Resolving each location
/// at link time and storing the handle drops the per-frame
/// `format!("u_param{i}") → get_uniform_location` round-trip down to a
/// single array index. Locations that don't appear in the shader become
/// `None` and writes to them are no-ops.
pub struct SceneProgram {
    pub program: glow::Program,
    u_time: Option<glow::UniformLocation>,
    u_resolution: Option<glow::UniformLocation>,
    u_audio: Option<glow::UniformLocation>,
    u_audio_mid: Option<glow::UniformLocation>,
    u_beat: Option<glow::UniformLocation>,
    u_trigger: Option<glow::UniformLocation>,
    u_bpm: Option<glow::UniformLocation>,
    u_params: [Option<glow::UniformLocation>; 9],
}

/// Cached uniform locations for the single blend program.
pub struct BlendProgram {
    pub program: glow::Program,
    u_xfade: Option<glow::UniformLocation>,
    u_blend_mode: Option<glow::UniformLocation>,
}

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
    blend: BlendProgram,
    quad_vao: glow::VertexArray,
    /// scene name → compiled program (with cached uniform locations)
    scene_programs: BTreeMap<String, SceneProgram>,
    /// scene name → preferred layer FBO size, parsed from
    /// `internal_resolution` in the scene's toml. Absent = use `width × height`.
    /// Ignored entirely on Pi 5 / Unknown (desktop) — those tiers have the
    /// GPU headroom to run every scene at native scanout, so the Pi-3-tuned
    /// caps would just waste pixels. See roadmap item 28a.
    scene_sizes: BTreeMap<String, (u32, u32)>,
    /// Detected Pi generation. Gates whether per-scene `internal_resolution`
    /// caps apply.
    pi_gen: PiGen,
    overlay_program: Option<glow::Program>,
    /// 1×320 RGBA8 texture mirroring `AudioHistory::snapshot_rgba`. Created
    /// up-front (zeros) so binding is always valid; main loop refreshes
    /// content each frame via `upload_audio_history`.
    audio_history_texture: Option<glow::Texture>,
    /// Post-FX chain. Empty by default; populated via `postfx_load_dir`.
    /// When any pass in the chain is enabled, the blend output is rerouted
    /// into the chain's input FBO and the last enabled pass writes to the
    /// swapchain.
    pub postfx: PostFx,
}

/// Resolve the layer-FBO size for one scene given the parsed
/// `internal_resolution` and the detected Pi gen. Pi 5 and the desktop dev
/// box (`Unknown`) always return `None` — they render at the global scanout
/// dims so previously down-scaled scenes scale up to the GPU's headroom.
/// Roadmap 28a.
fn resolved_scene_size(
    declared: Option<(u32, u32)>,
    pi_gen: PiGen,
) -> Option<(u32, u32)> {
    if pi_gen >= PiGen::Pi5 {
        return None;
    }
    declared
}

impl Pipeline {
    /// Backwards-compat constructor — assumes desktop dev (`PiGen::Unknown`).
    /// Production callers should use `new_for_gen` with the detected gen so
    /// per-scene resolution caps gate correctly.
    pub fn new(gl: Arc<glow::Context>, width: u32, height: u32) -> Result<Self> {
        Self::new_for_gen(gl, width, height, PiGen::Unknown)
    }

    pub fn new_for_gen(
        gl: Arc<glow::Context>,
        width: u32,
        height: u32,
        pi_gen: PiGen,
    ) -> Result<Self> {
        let fbo_a = [
            Fbo::new(gl.clone(), width, height)?,
            Fbo::new(gl.clone(), width, height)?,
        ];
        let fbo_b = [
            Fbo::new(gl.clone(), width, height)?,
            Fbo::new(gl.clone(), width, height)?,
        ];
        let blend_program = compile_program(&gl, QUAD_VERT, BLEND_FRAG)?;
        let blend = resolve_blend_uniforms(&gl, blend_program);
        let quad_vao = create_quad_vao(&gl)?;
        let audio_history_texture = create_audio_history_texture(&gl);
        let postfx = PostFx::new(gl.clone(), width, height)?;
        // One-time GL state setup that holds for the lifetime of the
        // process: bind the unit-2 audio history texture so it's always
        // there for scenes to sample, and bind the quad VAO once since
        // every draw in this pipeline uses the same single quad. Skipping
        // these per-frame saves ~6 GL calls each render tick.
        unsafe {
            if let Some(tex) = audio_history_texture {
                gl.active_texture(glow::TEXTURE2);
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.active_texture(glow::TEXTURE0);
            }
            gl.bind_vertex_array(Some(quad_vao));
        }
        Ok(Self {
            gl,
            width,
            height,
            fbo_a,
            fbo_b,
            front_a: 0,
            front_b: 0,
            blend,
            quad_vao,
            scene_programs: BTreeMap::new(),
            scene_sizes: BTreeMap::new(),
            pi_gen,
            overlay_program: None,
            audio_history_texture,
            postfx,
        })
    }

    /// Load every paired `<dir>/*.{glsl,toml}` as a post-FX pass. Safe to call
    /// at startup before any frames render; failures on individual passes are
    /// logged and skipped so a single broken shader doesn't kill the chain.
    pub fn postfx_load_dir(&mut self, dir: &std::path::Path) -> Result<()> {
        self.postfx.load_dir(dir)
    }

    /// Refresh the audio-history texture from a packed RGBA8 snapshot.
    /// `rgba` must be `1 * AUDIO_HISTORY_LEN * 4` bytes; mismatched lengths
    /// are silently skipped (test paths may pass empty buffers). Calls into
    /// `tex_sub_image_2d`; the texture itself is created once in `new()`.
    pub fn upload_audio_history(&mut self, rgba: &[u8]) {
        if self.audio_history_texture.is_none() {
            return;
        }
        if rgba.len() != AUDIO_HISTORY_LEN * 4 {
            return;
        }
        unsafe {
            // Texture is permanently bound to TEXTURE2 (set in `Pipeline::new`)
            // so we don't need an extra `bind_texture` here. The active
            // unit *might* be 0 after a render pass, so we flip to 2 to
            // target the right texture before `tex_sub_image_2d`, then
            // restore TEXTURE0 so subsequent code doesn't bind unrelated
            // textures into unit 2 by accident.
            self.gl.active_texture(glow::TEXTURE2);
            self.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                1,
                AUDIO_HISTORY_LEN as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(rgba),
            );
            self.gl.active_texture(glow::TEXTURE0);
        }
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
        let cached = resolve_scene_uniforms(&self.gl, new_prog);
        // Sampler bindings never change at runtime — set them once here and
        // never re-upload. `u_prev` reads from TEXTURE0 (the per-layer prev
        // FBO bound at render time); `u_audio_history` reads from TEXTURE2
        // (bound permanently in `Pipeline::new`).
        unsafe {
            self.gl.use_program(Some(new_prog));
            let loc_prev = self.gl.get_uniform_location(new_prog, "u_prev");
            if loc_prev.is_some() {
                self.gl.uniform_1_i32(loc_prev.as_ref(), 0);
            }
            let loc_aud = self.gl.get_uniform_location(new_prog, "u_audio_history");
            if loc_aud.is_some() {
                self.gl.uniform_1_i32(loc_aud.as_ref(), 2);
            }
        }
        if let Some(old) = self.scene_programs.insert(name.to_string(), cached) {
            unsafe { self.gl.delete_program(old.program) };
        }
        match resolved_scene_size(scene.meta.internal_resolution_size(), self.pi_gen) {
            Some(size) => {
                self.scene_sizes.insert(name.to_string(), size);
            }
            None => {
                self.scene_sizes.remove(name);
            }
        }
        Ok(())
    }

    /// Layer FBO size to use when rendering `scene_name`. Per-scene override
    /// wins; otherwise the global pipeline render-scale size applies.
    fn layer_size_for(&self, scene_name: &str) -> (u32, u32) {
        self.scene_sizes
            .get(scene_name)
            .copied()
            .unwrap_or((self.width, self.height))
    }

    /// Recreate `fbo_a`'s ping-pong pair at `(w, h)` if they currently differ.
    /// Resets `front_a` since the new FBOs are fresh (cleared to opaque black).
    fn ensure_layer_a_size(&mut self, w: u32, h: u32) -> Result<()> {
        if self.fbo_a[0].width == w && self.fbo_a[0].height == h {
            return Ok(());
        }
        self.fbo_a = [
            Fbo::new(self.gl.clone(), w, h)?,
            Fbo::new(self.gl.clone(), w, h)?,
        ];
        self.front_a = 0;
        Ok(())
    }

    fn ensure_layer_b_size(&mut self, w: u32, h: u32) -> Result<()> {
        if self.fbo_b[0].width == w && self.fbo_b[0].height == h {
            return Ok(());
        }
        self.fbo_b = [
            Fbo::new(self.gl.clone(), w, h)?,
            Fbo::new(self.gl.clone(), w, h)?,
        ];
        self.front_b = 0;
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
        let sp = self
            .scene_programs
            .get(scene_name)
            .ok_or_else(|| Error::SceneNotFound(scene_name.into()))?;
        target.bind();
        unsafe {
            self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
            self.gl.use_program(Some(sp.program));
            // Bind the prev-frame texture on the always-active TEXTURE0
            // unit. `u_prev` and `u_audio_history` samplers were set to
            // their unit indices when the program was linked, so no
            // per-frame `set_uniform_int` round-trip is needed. The
            // audio-history texture is bound permanently to TEXTURE2 in
            // `Pipeline::new`, so we never touch unit 2 here either.
            self.gl.bind_texture(glow::TEXTURE_2D, Some(prev.texture));
            if let Some(loc) = &sp.u_time {
                self.gl.uniform_1_f32(Some(loc), state.time_secs);
            }
            if let Some(loc) = &sp.u_resolution {
                self.gl.uniform_2_f32(Some(loc), self.width as f32, self.height as f32);
            }
            // Zero out audio bands when audio is bypassed so scenes can't
            // see leaked-through reactivity through the u_audio uniform.
            // Per-slot routing is already gated by `audio_bypass` inside
            // `effective_slot_values`, but `u_audio` was direct.
            let bands = if state.audio_bypass {
                [0.0; 5]
            } else {
                state.audio_bands
            };
            if let Some(loc) = &sp.u_audio {
                // u_audio.xyzw = [bass, lomid, himid, treble]. The new
                // mid band is uploaded separately into u_audio_mid below
                // so existing scenes that only sample u_audio keep
                // working unmodified.
                self.gl.uniform_4_f32(Some(loc), bands[0], bands[1], bands[2], bands[3]);
            }
            if let Some(loc) = &sp.u_audio_mid {
                self.gl.uniform_1_f32(Some(loc), bands[4]);
            }
            if let Some(loc) = &sp.u_trigger {
                self.gl.uniform_1_f32(Some(loc), state.trigger);
            }
            let beat_uniform = if state.audio_bypass { 0.0 } else { state.trigger };
            if let Some(loc) = &sp.u_beat {
                self.gl.uniform_1_f32(Some(loc), beat_uniform);
            }
            if let Some(loc) = &sp.u_bpm {
                self.gl.uniform_1_f32(Some(loc), state.tap_tempo_bpm);
            }
            let slots = params.effective_slot_values(&state.audio_bands, state.audio_bypass);
            for (i, v) in slots.iter().enumerate() {
                if let Some(loc) = &sp.u_params[i] {
                    self.gl.uniform_1_f32(Some(loc), *v);
                }
            }
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
            self.render_blend(state);
        }
    }

    /// Render the blend pass into the currently-bound framebuffer + viewport.
    /// `render_blend_to_default` is the swapchain case; the post-FX path
    /// binds `postfx.input_framebuffer()` first and then calls this.
    unsafe fn render_blend(&self, state: &SharedState) {
        self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
        self.gl.clear(glow::COLOR_BUFFER_BIT);
        self.gl.use_program(Some(self.blend.program));
        self.gl.active_texture(glow::TEXTURE0);
        self.gl
            .bind_texture(glow::TEXTURE_2D, Some(self.fbo_a[self.front_a].texture));
        self.gl.active_texture(glow::TEXTURE1);
        self.gl
            .bind_texture(glow::TEXTURE_2D, Some(self.fbo_b[self.front_b].texture));
        // Sampler bindings (u_layer_a→0, u_layer_b→1) were set at link
        // time in `Pipeline::new` via `resolve_blend_uniforms`, so we only
        // need to upload xfade + blend mode here.
        if let Some(loc) = &self.blend.u_xfade {
            self.gl.uniform_1_f32(Some(loc), state.xfade);
        }
        if let Some(loc) = &self.blend.u_blend_mode {
            self.gl.uniform_1_i32(Some(loc), state.blend_mode.as_int());
        }
        // Restore TEXTURE0 as the active unit so the next scene draw
        // doesn't accidentally bind its prev FBO into TEXTURE1.
        self.gl.active_texture(glow::TEXTURE0);
        self.gl.draw_arrays(glow::TRIANGLES, 0, VERTEX_COUNT);
    }

    pub fn frame(
        &mut self,
        state: &SharedState,
        default_fb_w: u32,
        default_fb_h: u32,
    ) -> Result<()> {
        // Layer-skip optimization: blend pass mixes A and B by xfade. When a
        // layer's weight is zero, its frame contributes nothing — skip its
        // scene shader. Threshold below 1/255 since the blend output is 8-bit.
        let need_a = state.xfade < 0.999;
        let need_b = state.xfade > 0.001;

        if need_a {
            let (w, h) = self.layer_size_for(&state.layer_a.scene_name);
            self.ensure_layer_a_size(w, h)?;
            let next_a = 1 - self.front_a;
            self.render_layer_to(
                &self.fbo_a[next_a],
                &self.fbo_a[self.front_a],
                &state.layer_a.scene_name,
                &state.layer_a.params,
                state,
            )?;
            self.front_a = next_a;
        }

        if need_b {
            let (w, h) = self.layer_size_for(&state.layer_b.scene_name);
            self.ensure_layer_b_size(w, h)?;
            let next_b = 1 - self.front_b;
            self.render_layer_to(
                &self.fbo_b[next_b],
                &self.fbo_b[self.front_b],
                &state.layer_b.scene_name,
                &state.layer_b.params,
                state,
            )?;
            self.front_b = next_b;
        }

        if self.postfx.has_enabled() {
            // Blend → first ping-pong FBO, then walk the chain. The last
            // enabled pass targets the swapchain itself, so we never read
            // the GPU output back into a redundant FBO copy.
            {
                let target = self.postfx.input_framebuffer();
                target.bind();
                unsafe {
                    self.render_blend(state);
                }
            }
            self.postfx
                .run(state, default_fb_w, default_fb_h, self.quad_vao);
        } else {
            self.render_blend_to_default(default_fb_w, default_fb_h, state);
        }
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

/// Look up every uniform a scene shader can reference and store the
/// `UniformLocation` handles up front. Called once per `upsert_scene`; the
/// resulting struct lives for the program's lifetime. Missing uniforms
/// (e.g. a shader that doesn't use `u_beat`) are stored as `None` so the
/// per-frame write is a 1-byte branch instead of a string round-trip into
/// the GL driver.
fn resolve_scene_uniforms(gl: &glow::Context, program: glow::Program) -> SceneProgram {
    let loc = |name: &str| unsafe { gl.get_uniform_location(program, name) };
    let u_params = std::array::from_fn(|i| loc(&format!("u_param{i}")));
    SceneProgram {
        program,
        u_time: loc("u_time"),
        u_resolution: loc("u_resolution"),
        u_audio: loc("u_audio"),
        u_audio_mid: loc("u_audio_mid"),
        u_beat: loc("u_beat"),
        u_trigger: loc("u_trigger"),
        u_bpm: loc("u_bpm"),
        u_params,
    }
}

/// Same idea for the singleton blend program. Samplers `u_layer_a` and
/// `u_layer_b` get their unit assignments set here too so the per-frame
/// blend dispatch is just xfade + blend_mode + a draw call.
fn resolve_blend_uniforms(gl: &glow::Context, program: glow::Program) -> BlendProgram {
    let loc = |name: &str| unsafe { gl.get_uniform_location(program, name) };
    let u_layer_a = loc("u_layer_a");
    let u_layer_b = loc("u_layer_b");
    unsafe {
        gl.use_program(Some(program));
        if u_layer_a.is_some() {
            gl.uniform_1_i32(u_layer_a.as_ref(), 0);
        }
        if u_layer_b.is_some() {
            gl.uniform_1_i32(u_layer_b.as_ref(), 1);
        }
    }
    BlendProgram {
        program,
        u_xfade: loc("u_xfade"),
        u_blend_mode: loc("u_blend_mode"),
    }
}

pub(super) fn compile_program(gl: &glow::Context, vert: &str, frag: &str) -> Result<glow::Program> {
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

// One-shot uniform setters used by the overlay path. Hot paths (scene,
// blend, post-FX) cache uniform locations at link time and never call
// these.
unsafe fn set_uniform_int(gl: &glow::Context, prog: glow::Program, name: &str, v: i32) {
    let loc = gl.get_uniform_location(prog, name);
    gl.uniform_1_i32(loc.as_ref(), v);
}
unsafe fn set_uniform_vec2(gl: &glow::Context, prog: glow::Program, name: &str, x: f32, y: f32) {
    let loc = gl.get_uniform_location(prog, name);
    gl.uniform_2_f32(loc.as_ref(), x, y);
}

const OVERLAY_VERT: &str = include_str!("../../shaders/quad.vert");
const OVERLAY_FRAG: &str = include_str!("../../shaders/overlay.glsl");

/// Mirrors `audio::history::HISTORY_LEN`. Duplicated here to avoid a circular
/// dep into the audio module from render; CI test below pins them together.
pub const AUDIO_HISTORY_LEN: usize = 320;

fn create_audio_history_texture(gl: &glow::Context) -> Option<glow::Texture> {
    unsafe {
        let tex = gl.create_texture().ok()?;
        gl.active_texture(glow::TEXTURE2);
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        let zeros = vec![0u8; AUDIO_HISTORY_LEN * 4];
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA as i32,
            1,
            AUDIO_HISTORY_LEN as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            Some(&zeros),
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::NEAREST as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::NEAREST as i32,
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
        gl.active_texture(glow::TEXTURE0);
        Some(tex)
    }
}

#[cfg(test)]
#[cfg(feature = "desktop")]
mod tests {
    use super::*;
    use crate::render::desktop::WinitGlTarget;
    use crate::render::target::RenderTarget;
    use crate::scene::{LoadedScene, SceneMeta};
    use std::sync::Arc;

    #[test]
    fn audio_history_constant_matches_audio_module() {
        assert_eq!(AUDIO_HISTORY_LEN, crate::audio::history::HISTORY_LEN);
    }

    #[test]
    fn resolved_scene_size_honours_caps_below_pi5() {
        // Pi 3 / Pi 4 honour the per-scene cap (Pi-3 tuning era).
        assert_eq!(
            super::resolved_scene_size(Some((180, 120)), PiGen::Pi3),
            Some((180, 120))
        );
        assert_eq!(
            super::resolved_scene_size(Some((180, 120)), PiGen::Pi4),
            Some((180, 120))
        );
    }

    #[test]
    fn resolved_scene_size_drops_caps_on_pi5_and_unknown() {
        // Pi 5 and the desktop dev box (Unknown) ignore the per-scene cap so
        // previously down-scaled scenes scale up to native scanout.
        assert_eq!(super::resolved_scene_size(Some((180, 120)), PiGen::Pi5), None);
        assert_eq!(
            super::resolved_scene_size(Some((180, 120)), PiGen::Unknown),
            None
        );
        // Absent cap stays absent regardless of gen.
        assert_eq!(super::resolved_scene_size(None, PiGen::Pi3), None);
    }

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
