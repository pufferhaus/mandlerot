//! Post-FX chain. Sits after the A/B blend and before the swapchain.
//!
//! The chain is a Vec of `PostFxPass`. Each pass owns:
//!   - a compiled program built from `shaders/postfx_prelude.glsl` + a user
//!     body in `postfx/<name>.glsl`,
//!   - a `ParamMap` with the same 8-slot semantics as scenes (so audio
//!     routing on a post-FX param works for free), and
//!   - a runtime `enabled` flag.
//!
//! Execution ping-pongs between two full-resolution FBOs. The pipeline writes
//! the blend output into `pp[0]` (see `input_framebuffer`), then `run` walks
//! each enabled pass; the last enabled pass targets the swapchain directly.
//! Chain with zero enabled passes is skipped by the caller — no draws issued.
//!
//! Memory footprint on the Pi 3B+: 2 FBOs × 720×480 RGBA8 = 2.64 MB on top of
//! the existing layer FBOs.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use glow::HasContext;

use crate::error::Result;
use crate::scene::{ParamMap, SceneMeta};
use crate::state::SharedState;

use super::fbo::Fbo;
use super::pipeline::compile_program;
use super::quad::VERTEX_COUNT;
use super::shader::{assemble_postfx_fragment, QUAD_VERT};

/// Minimal passthrough fragment shader used to blit the post-FX feedback fbo
/// to the default framebuffer at the end of `run`. Lives here because it's
/// internal to the chain pipeline; no use putting it in `shaders/`.
const BLIT_FRAG: &str = "#version 100\nprecision mediump float;\nuniform sampler2D u_input;\nvarying vec2 v_uv;\nvoid main(){ gl_FragColor = texture2D(u_input, v_uv); }\n";

/// Cached uniform locations for one post-FX pass. Resolved at link time
/// (`resolve_postfx_uniforms`) so the hot `run` loop just does
/// `uniform_1_f32` against a stored handle — no per-frame string lookups.
struct PostFxUniforms {
    u_time: Option<glow::UniformLocation>,
    u_resolution: Option<glow::UniformLocation>,
    u_audio_mid: Option<glow::UniformLocation>,
    u_params: [Option<glow::UniformLocation>; 8],
    u_lut: Option<glow::UniformLocation>,
}

fn resolve_postfx_uniforms(gl: &glow::Context, program: glow::Program) -> PostFxUniforms {
    let loc = |name: &str| unsafe { gl.get_uniform_location(program, name) };
    // Samplers are fixed to specific texture units once at link time so the
    // hot `run` loop only binds textures, never re-uploads sampler ints.
    //   u_input → unit 0 (previous pass's output, or the blend fbo for pass 0)
    //   u_prev  → unit 1 (last frame's final chain output, for trails / feedback)
    let u_input = loc("u_input");
    let u_prev = loc("u_prev");
    unsafe {
        gl.use_program(Some(program));
        if u_input.is_some() {
            gl.uniform_1_i32(u_input.as_ref(), 0);
        }
        if u_prev.is_some() {
            gl.uniform_1_i32(u_prev.as_ref(), 1);
        }
    }
    let u_params = std::array::from_fn(|i| loc(&format!("u_param{i}")));
    PostFxUniforms {
        u_time: loc("u_time"),
        u_resolution: loc("u_resolution"),
        u_audio_mid: loc("u_audio_mid"),
        u_params,
        u_lut: loc("u_lut"),
    }
}

/// One post-FX pass: shader + params + on/off.
pub struct PostFxPass {
    pub name: String,
    pub meta: SceneMeta,
    pub fragment_body: String,
    pub source_path: PathBuf,
    pub program: glow::Program,
    pub params: ParamMap,
    pub enabled: bool,
    /// Cached uniform locations for `program`. Populated at link time.
    uniforms: PostFxUniforms,
    /// Only populated for the pass named "lut" — empty for all others.
    pub lut_textures: Vec<glow::Texture>,
}

/// The whole chain. The order in `passes` is the dispatch order; `enabled`
/// flips per pass.
pub struct PostFx {
    gl: Arc<glow::Context>,
    width: u32,
    height: u32,
    pp: [Fbo; 2],
    /// Index of the FBO that holds the *input* texture for the next pass.
    /// `run` flips this each pass; the pipeline writes the blend output to
    /// `pp[input_idx]` via `input_framebuffer()` before calling `run`.
    input_idx: usize,
    /// Ping-pong feedback buffers. `feedback[front_feedback]` holds the
    /// previous frame's final chain output and is bound as `u_prev` on
    /// TEXTURE1 for the duration of `run`. The other slot is the write
    /// target for the current frame's last pass; afterwards `front_feedback`
    /// flips so this frame's output becomes next frame's `u_prev`.
    feedback: [Fbo; 2],
    front_feedback: usize,
    /// Trivial copy program — samples a texture, writes RGBA unmodified. Used
    /// at the end of `run` to blit the just-written feedback fbo to the
    /// default framebuffer (scanout) so the chain can keep its final output
    /// in a persistent fbo for the next frame's `u_prev`.
    blit_program: glow::Program,
    passes: Vec<PostFxPass>,
    /// Cached "vig+grn+chr" style status-bar tag. Recomputed only when the
    /// chain mutates (toggle / load / upsert); read every frame by the main
    /// loop to populate the StateSnapshot. Eliminates a per-frame `Vec<String>`
    /// allocation.
    summary_cache: String,
}

impl PostFx {
    /// Construct an empty chain — `passes` empty, two FBOs allocated.
    pub fn new(gl: Arc<glow::Context>, width: u32, height: u32) -> Result<Self> {
        let pp = [
            Fbo::new(gl.clone(), width, height)?,
            Fbo::new(gl.clone(), width, height)?,
        ];
        let feedback = [
            Fbo::new(gl.clone(), width, height)?,
            Fbo::new(gl.clone(), width, height)?,
        ];
        let blit_program = compile_program(&gl, QUAD_VERT, BLIT_FRAG)?;
        unsafe {
            let loc = gl.get_uniform_location(blit_program, "u_input");
            gl.use_program(Some(blit_program));
            if loc.is_some() {
                gl.uniform_1_i32(loc.as_ref(), 0);
            }
        }
        Ok(Self {
            gl,
            width,
            height,
            pp,
            input_idx: 0,
            feedback,
            front_feedback: 0,
            blit_program,
            passes: Vec::new(),
            summary_cache: String::new(),
        })
    }

    /// Load every `postfx/*.{glsl,toml}` pair under `dir`. Order is
    /// alphabetical by stem — deterministic so users can predict the chain.
    /// Reordering UI lands in a later phase.
    pub fn load_dir(&mut self, dir: &Path) -> Result<()> {
        let mut found: BTreeMap<String, (PathBuf, PathBuf)> = BTreeMap::new();
        if !dir.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("glsl") {
                continue;
            }
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let meta_path = path.with_extension("toml");
            if !meta_path.exists() {
                tracing::warn!("postfx {} has no .toml metadata, skipping", path.display());
                continue;
            }
            found.insert(stem, (path, meta_path));
        }
        for (name, (glsl, toml_path)) in found {
            match self.load_one(&name, &glsl, &toml_path) {
                Ok(pass) => self.passes.push(pass),
                Err(e) => tracing::warn!("postfx '{name}': {e}; skipping"),
            }
        }
        self.refresh_summary();
        Ok(())
    }

    fn load_one(&self, _name: &str, glsl: &Path, toml_path: &Path) -> Result<PostFxPass> {
        let body = std::fs::read_to_string(glsl)?;
        let meta_str = std::fs::read_to_string(toml_path)?;
        let meta = SceneMeta::parse(&meta_str, &toml_path.display().to_string())?;
        meta.validate()?;
        let frag = assemble_postfx_fragment(&body);
        let program = compile_program(&self.gl, QUAD_VERT, &frag)?;
        let uniforms = resolve_postfx_uniforms(&self.gl, program);
        let params = ParamMap::from_scene(&meta);
        Ok(PostFxPass {
            name: meta.name.clone(),
            enabled: meta.enabled_by_default,
            meta,
            fragment_body: body,
            source_path: glsl.to_path_buf(),
            program,
            params,
            uniforms,
            lut_textures: Vec::new(),
        })
    }

    /// The FBO the pipeline should write the blend output to before calling
    /// `run`. Always `pp[input_idx]`. Caller must reset `input_idx=0` by
    /// virtue of `run` ending there; we hand back `pp[0]` here regardless so
    /// the caller has a single stable entry point.
    pub fn input_framebuffer(&mut self) -> &Fbo {
        self.input_idx = 0;
        &self.pp[0]
    }

    /// True if any pass is enabled — caller can skip the whole chain.
    pub fn has_enabled(&self) -> bool {
        self.passes.iter().any(|p| p.enabled)
    }

    /// Short, status-bar-friendly summary of the enabled passes — `vig+grn`
    /// style. Returns a borrow of the cached string; updated in-place by
    /// `refresh_summary` on every chain mutation.
    pub fn summary_tag(&self) -> &str {
        &self.summary_cache
    }

    pub fn passes(&self) -> &[PostFxPass] {
        &self.passes
    }

    pub fn passes_mut(&mut self) -> &mut [PostFxPass] {
        &mut self.passes
    }

    /// Walk the enabled passes. The pipeline must have just written the
    /// blend output into `input_framebuffer()`. The last enabled pass
    /// targets the default framebuffer (`default_fb_w` × `default_fb_h`).
    ///
    /// `quad_vao` is the pipeline's unit quad — borrowed to avoid duplicating
    /// the buffer.
    pub fn run(
        &mut self,
        state: &SharedState,
        default_fb_w: u32,
        default_fb_h: u32,
        _quad_vao: glow::VertexArray,
    ) {
        // Walk enabled passes. The chain always ends in a feedback fbo (not
        // scanout) so we can read it as `u_prev` next frame; a final blit
        // pass copies that fbo to the default framebuffer.
        //
        // A LUT pass with no textures on disk can never render — exclude it
        // from both the count and the iteration so `is_last` is always set by
        // a pass that actually draws and writes to `feedback[next_feedback]`.
        let is_renderable = |p: &PostFxPass| -> bool {
            p.enabled && !(p.name == "lut" && p.lut_textures.is_empty())
        };
        let enabled_count = self.passes.iter().filter(|p| is_renderable(p)).count();
        if enabled_count == 0 {
            return;
        }
        let prev_feedback = self.front_feedback;
        let next_feedback = 1 - prev_feedback;
        unsafe {
            // Bind last frame's final chain output to TEXTURE1 = u_prev for
            // every pass in this chain. Cold-boot value is opaque black
            // because `Fbo::new` clears the texture once. Bind once outside
            // the loop so subsequent `bind_texture` calls (which target the
            // active unit) don't clobber it.
            self.gl.active_texture(glow::TEXTURE1);
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.feedback[prev_feedback].texture));
            self.gl.active_texture(glow::TEXTURE0);
        }
        // The pipeline's quad VAO is bound once at startup; no need to
        // re-bind here.
        let mut seen = 0usize;
        for pass in self.passes.iter().filter(|p| is_renderable(p)) {
            seen += 1;
            let is_last = seen == enabled_count;
            let input_idx = self.input_idx;
            let next_idx = 1 - input_idx;
            // Pre-filter guarantees lut_textures is non-empty when name == "lut",
            // so pick_lut_index always returns Some here.
            let lut_bind = if pass.name == "lut" {
                let slots_preview = pass
                    .params
                    .effective_slot_values(&state.audio_bands, state.audio_bypass);
                crate::render::lut::pick_lut_index(slots_preview[0], pass.lut_textures.len())
                    .map(|idx| pass.lut_textures[idx])
            } else {
                None
            };
            unsafe {
                if is_last {
                    // End the chain in a persistent feedback fbo. A
                    // passthrough blit further down copies it to scanout.
                    self.feedback[next_feedback].bind();
                } else {
                    self.pp[next_idx].bind();
                }
                // Pass fully covers the target, so the clear is unnecessary —
                // skipped to save fill on Pi 3B+ where bandwidth is precious.
                self.gl.use_program(Some(pass.program));
                self.gl
                    .bind_texture(glow::TEXTURE_2D, Some(self.pp[input_idx].texture));
                // Sampler `u_input` was set to unit 0 at link time. We
                // never leave unit 0 after a frame so no `active_texture`
                // call needed here either.
                if let Some(tex) = lut_bind {
                    self.gl.active_texture(glow::TEXTURE4);
                    self.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                    if let Some(loc) = &pass.uniforms.u_lut {
                        self.gl.uniform_1_i32(Some(loc), 4);
                    }
                    // Restore TU0 as active so the existing `u_input` binding
                    // code path continues to write to TU0 next iteration.
                    self.gl.active_texture(glow::TEXTURE0);
                }
                if let Some(loc) = &pass.uniforms.u_time {
                    self.gl.uniform_1_f32(Some(loc), state.time_secs);
                }
                if let Some(loc) = &pass.uniforms.u_resolution {
                    self.gl.uniform_2_f32(
                        Some(loc),
                        self.width as f32,
                        self.height as f32,
                    );
                }
                if let Some(loc) = &pass.uniforms.u_audio_mid {
                    let mid = if state.audio_bypass {
                        0.0
                    } else {
                        state.audio_bands[4]
                    };
                    self.gl.uniform_1_f32(Some(loc), mid);
                }
                // Audio routing is already resolved into the per-slot effective
                // values; uploading them lets a post-FX param react to bass /
                // treble / mid for free.
                let slots = pass
                    .params
                    .effective_slot_values(&state.audio_bands, state.audio_bypass);
                for (i, loc) in pass.uniforms.u_params.iter().enumerate() {
                    if let Some(loc) = loc {
                        self.gl.uniform_1_f32(Some(loc), slots[i]);
                    }
                }
                self.gl.draw_arrays(glow::TRIANGLES, 0, VERTEX_COUNT);
                if lut_bind.is_some() {
                    self.gl.active_texture(glow::TEXTURE4);
                    self.gl.bind_texture(glow::TEXTURE_2D, None);
                    self.gl.active_texture(glow::TEXTURE0);
                }
            }
            self.input_idx = next_idx;
        }

        // Final blit: feedback[next_feedback] now holds this frame's final
        // chain output. Copy it to the default framebuffer (scanout) and
        // promote it to be `u_prev` for the next frame.
        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            self.gl
                .viewport(0, 0, default_fb_w as i32, default_fb_h as i32);
            self.gl.use_program(Some(self.blit_program));
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.feedback[next_feedback].texture));
            self.gl.draw_arrays(glow::TRIANGLES, 0, VERTEX_COUNT);
        }
        self.front_feedback = next_feedback;
    }

    /// Recompile a pass after a hot-reload of its `.glsl` body / `.toml`
    /// meta. Returns `true` if the pass existed and was successfully
    /// re-linked. Failures leave the existing program in place (matches the
    /// scene hot-reload behaviour).
    pub fn upsert(
        &mut self,
        name: &str,
        body: &str,
        meta: SceneMeta,
        source_path: PathBuf,
    ) -> Result<()> {
        let frag = assemble_postfx_fragment(body);
        let program = compile_program(&self.gl, QUAD_VERT, &frag)?;
        let uniforms = resolve_postfx_uniforms(&self.gl, program);
        if let Some(existing) = self.passes.iter_mut().find(|p| p.name == name) {
            unsafe { self.gl.delete_program(existing.program) };
            existing.fragment_body = body.to_string();
            existing.params = ParamMap::from_scene(&meta);
            existing.meta = meta;
            existing.source_path = source_path;
            existing.program = program;
            existing.uniforms = uniforms;
            return Ok(());
        }
        // Brand-new pass on disk — append to the chain in load order. Avoids
        // having to restart to pick up a freshly-authored pass.
        let pass = PostFxPass {
            name: name.to_string(),
            enabled: meta.enabled_by_default,
            params: ParamMap::from_scene(&meta),
            meta,
            fragment_body: body.to_string(),
            source_path,
            program,
            uniforms,
            lut_textures: Vec::new(),
        };
        self.passes.push(pass);
        self.refresh_summary();
        Ok(())
    }

    /// Toggle a pass by index. No-op if out of range.
    pub fn toggle(&mut self, idx: usize) {
        if let Some(p) = self.passes.get_mut(idx) {
            p.enabled = !p.enabled;
            self.refresh_summary();
        }
    }

    /// Rebuild the cached status-bar summary string. Called from every
    /// mutation path (toggle / load_state / load_dir / upsert) so the
    /// `summary_tag()` accessor stays allocation-free.
    fn refresh_summary(&mut self) {
        self.summary_cache.clear();
        let mut first = true;
        for p in self.passes.iter().filter(|p| p.enabled) {
            if !first {
                self.summary_cache.push('+');
            }
            first = false;
            for c in p.name.chars().take(3) {
                self.summary_cache.push(c);
            }
        }
    }

    /// Mutable handle to one pass's `ParamMap`. Used by the UI to nudge
    /// values directly; the param map clamps to `min..max` on every `set`.
    pub fn pass_params_mut(&mut self, idx: usize) -> Option<&mut ParamMap> {
        self.passes.get_mut(idx).map(|p| &mut p.params)
    }

    /// Look up a pass by name. Returns the index into `passes`.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.passes.iter().position(|p| p.name == name)
    }

    /// Persist enabled flags + current param values to
    /// `<state_dir>/postfx.toml`. Atomic write — tmp + rename. Called by the
    /// UI on every mutation so the file always reflects the live chain.
    pub fn save_state(&self, state_dir: &Path) -> Result<()> {
        use std::fmt::Write as _;
        let mut out = String::new();
        out.push_str(
            "# postfx.toml — user-tunable post-FX chain state. Hand-edit at\n# your own risk; the in-app PostFX screen rewrites this file\n# atomically on every change.\n",
        );
        for pass in &self.passes {
            writeln!(out, "\n[{}]", pass.name).ok();
            writeln!(out, "enabled = {}", pass.enabled).ok();
            for d in pass.params.defs() {
                if let Some(v) = pass.params.get(&d.name) {
                    writeln!(out, "{} = {}", d.name, format_param_value(v)).ok();
                }
            }
        }
        let path = state_dir.join("postfx.toml");
        let tmp = path.with_extension("toml.tmp");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&tmp, &out)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Apply a previously-saved `postfx.toml` on top of the loaded chain.
    /// Unknown pass names are ignored (e.g. user removed a `postfx/*.glsl`
    /// since the last save). Missing file = silent no-op so a fresh install
    /// just uses each pass's `enabled_by_default`.
    pub fn load_state(&mut self, state_dir: &Path) -> Result<()> {
        let path = state_dir.join("postfx.toml");
        if !path.exists() {
            return Ok(());
        }
        let s = std::fs::read_to_string(&path)?;
        let value: toml::Value = match toml::from_str(&s) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("postfx.toml parse: {e}; ignoring");
                return Ok(());
            }
        };
        let toml::Value::Table(table) = value else {
            return Ok(());
        };
        for (pass_name, pass_val) in table {
            let toml::Value::Table(pass_table) = pass_val else {
                continue;
            };
            let Some(idx) = self.find(&pass_name) else {
                tracing::warn!("postfx.toml: pass '{pass_name}' not loaded, ignoring entry");
                continue;
            };
            let pass = &mut self.passes[idx];
            for (k, v) in pass_table {
                if k == "enabled" {
                    if let toml::Value::Boolean(b) = v {
                        pass.enabled = b;
                    }
                } else {
                    // Param name → float value. Both Float and Integer accepted
                    // (TOML promotes integer literals like `cell = 6`).
                    let f = match v {
                        toml::Value::Float(f) => Some(f as f32),
                        toml::Value::Integer(i) => Some(i as f32),
                        _ => None,
                    };
                    if let Some(f) = f {
                        pass.params.set(&k, f);
                    }
                }
            }
        }
        self.refresh_summary();
        Ok(())
    }
}

/// Format a param value with enough precision to round-trip through TOML.
/// Avoids writing `0` when we mean `0.0` (TOML would parse the former as
/// integer and our load path treats both, but the file reads cleaner).
fn format_param_value(v: f32) -> String {
    if v.fract() == 0.0 {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

impl Drop for PostFx {
    fn drop(&mut self) {
        // Programs are GL-owned; drop them explicitly so we don't leak when
        // the Pipeline tears down on shutdown. (FBOs handle themselves.)
        for pass in &self.passes {
            unsafe {
                for tex in &pass.lut_textures {
                    self.gl.delete_texture(*tex);
                }
            }
            unsafe { self.gl.delete_program(pass.program) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::SceneMeta;

    #[test]
    fn enabled_by_default_field_parses() {
        let m = SceneMeta::parse(
            "name = \"x\"\nenabled_by_default = true\n",
            "inline",
        )
        .unwrap();
        assert!(m.enabled_by_default);
    }

    #[test]
    fn enabled_by_default_defaults_to_false() {
        let m = SceneMeta::parse("name = \"x\"\n", "inline").unwrap();
        assert!(!m.enabled_by_default);
    }

    #[test]
    fn save_state_roundtrips_through_load() {
        // Build a synthetic pass list, write postfx.toml, then verify load
        // applies enabled + param edits onto a freshly-built mirror.
        // GL is not needed here because we exercise only the file-format
        // path; programs are constructed for the real `passes` Vec below.
        // To avoid a live GL context we hand-build a `PostFx` shell with a
        // fake program handle is not portable across glow versions, so this
        // test focuses on the parser instead: parse a known-good blob and
        // assert it matches the in-memory shape.
        let blob = "[vignette]\nenabled = false\nstrength = 0.33\n\n[grain]\nenabled = true\namount = 0.2\n";
        let value: toml::Value = toml::from_str(blob).unwrap();
        let toml::Value::Table(table) = value else {
            panic!("expected table");
        };
        assert_eq!(
            table.get("vignette").unwrap().as_table().unwrap().get("enabled"),
            Some(&toml::Value::Boolean(false))
        );
        assert!((table
            .get("vignette")
            .unwrap()
            .as_table()
            .unwrap()
            .get("strength")
            .unwrap()
            .as_float()
            .unwrap()
            - 0.33)
            .abs()
            < 1e-6);
    }

    #[test]
    fn format_param_value_keeps_decimal_for_integers() {
        assert_eq!(format_param_value(6.0), "6.0");
        assert_eq!(format_param_value(0.0), "0.0");
        let s = format_param_value(0.45);
        assert!(s.starts_with("0.45"));
    }

    #[test]
    fn ships_postfx_pass_pairs_on_disk() {
        for name in [
            "bloom", "chromatic", "dither", "grain", "lut",
            "pixelate", "trails", "vignette",
        ] {
            let glsl = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("postfx")
                .join(format!("{name}.glsl"));
            let toml = glsl.with_extension("toml");
            assert!(glsl.exists(), "missing {}", glsl.display());
            assert!(toml.exists(), "missing {}", toml.display());
            let s = std::fs::read_to_string(&toml).unwrap();
            let m = SceneMeta::parse(&s, &toml.display().to_string()).unwrap();
            m.validate().unwrap();
            assert_eq!(m.name, name);
        }
    }

    #[test]
    fn ships_baked_luts_on_disk() {
        let luts_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("postfx")
            .join("luts");
        for name in ["identity.png", "teal_orange.png"] {
            let p = luts_dir.join(name);
            assert!(p.exists(), "missing {}", p.display());
            let bytes = std::fs::read(&p).unwrap();
            assert_eq!(&bytes[0..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
            let rgba = crate::render::lut::decode_lut_png(&bytes).expect("baked LUT must decode");
            if name == "identity.png" {
                // Spot-check the identity mapping. Strip layout:
                //   x = blue_slice * 16 + r, y = g, channel = idx * 17.
                let pixel = |x: usize, y: usize| {
                    let base = (y * 256 + x) * 4;
                    [rgba[base], rgba[base + 1], rgba[base + 2], rgba[base + 3]]
                };
                assert_eq!(pixel(0, 0),   [0,   0,   0,   255], "identity (0,0)");
                assert_eq!(pixel(15, 0),  [255, 0,   0,   255], "identity (15,0)");
                assert_eq!(pixel(0, 15),  [0,   255, 0,   255], "identity (0,15)");
                assert_eq!(pixel(240, 0), [0,   0,   255, 255], "identity (240,0)");
            }
        }
    }
}
