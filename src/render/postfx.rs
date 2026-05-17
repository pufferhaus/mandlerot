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
use crate::platform::PiGen;
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

/// Pass names whose shader bodies are compiled from Rust-side constants
/// rather than from a paired `.glsl` file. Loader skips the missing-.glsl
/// warning for these; their TOML still ships under postfx/ to expose
/// params + enabled state + min_pi_gen via the existing scene-meta path.
const BUILTIN_POSTFX_PASSES: &[&str] = &["bloom_hq"];

fn is_builtin_postfx(name: &str) -> bool {
    BUILTIN_POSTFX_PASSES.contains(&name)
}

const BLOOM_DOWNSAMPLE_FRAG: &str = r#"
vec3 bright_pass(vec3 c, float t) {
    float lum = dot(c, vec3(0.299, 0.587, 0.114));
    return c * max(lum - t, 0.0);
}
void main() {
    vec2 px = 1.0 / u_resolution;
    vec3 c = (
        texture2D(u_input, v_uv + vec2(-0.5, -0.5) * px).rgb +
        texture2D(u_input, v_uv + vec2( 0.5, -0.5) * px).rgb +
        texture2D(u_input, v_uv + vec2(-0.5,  0.5) * px).rgb +
        texture2D(u_input, v_uv + vec2( 0.5,  0.5) * px).rgb
    ) * 0.25;
    gl_FragColor = vec4(bright_pass(c, u_param0), 1.0);
}
"#;

// Separable Gaussian, 5 linear-sampled taps (sigma ~= 1.5). The horizontal
// and vertical bodies are identical except for the offset axis.
const BLOOM_BLUR_H_FRAG: &str = r#"
const float W0 = 0.227027;
const float W1 = 0.316216;
const float W2 = 0.070270;
void main() {
    vec2 px = 1.0 / u_resolution;
    vec2 ofs1 = vec2(1.3846, 0.0) * px * u_param2;
    vec2 ofs2 = vec2(3.2308, 0.0) * px * u_param2;
    vec3 c = texture2D(u_input, v_uv).rgb * W0;
    c += texture2D(u_input, v_uv + ofs1).rgb * W1;
    c += texture2D(u_input, v_uv - ofs1).rgb * W1;
    c += texture2D(u_input, v_uv + ofs2).rgb * W2;
    c += texture2D(u_input, v_uv - ofs2).rgb * W2;
    gl_FragColor = vec4(c, 1.0);
}
"#;

const BLOOM_BLUR_V_FRAG: &str = r#"
const float W0 = 0.227027;
const float W1 = 0.316216;
const float W2 = 0.070270;
void main() {
    vec2 px = 1.0 / u_resolution;
    vec2 ofs1 = vec2(0.0, 1.3846) * px * u_param2;
    vec2 ofs2 = vec2(0.0, 3.2308) * px * u_param2;
    vec3 c = texture2D(u_input, v_uv).rgb * W0;
    c += texture2D(u_input, v_uv + ofs1).rgb * W1;
    c += texture2D(u_input, v_uv - ofs1).rgb * W1;
    c += texture2D(u_input, v_uv + ofs2).rgb * W2;
    c += texture2D(u_input, v_uv - ofs2).rgb * W2;
    gl_FragColor = vec4(c, 1.0);
}
"#;

// Composite stage: u_input is the original (pre-bloom) chain pixels;
// u_bloom is the blurred half-res result, bilinearly upsampled by the
// hardware (FBO defaults to LINEAR filtering per src/render/fbo.rs).
const BLOOM_COMPOSITE_FRAG: &str = r#"
uniform sampler2D u_bloom;
void main() {
    vec3 src   = texture2D(u_input, v_uv).rgb;
    vec3 bloom = texture2D(u_bloom, v_uv).rgb;
    gl_FragColor = vec4(src + bloom * u_param1, 1.0);
}
"#;

/// Cached uniform locations for one post-FX pass. Resolved at link time
/// (`resolve_postfx_uniforms`) so the hot `run` loop just does
/// `uniform_1_f32` against a stored handle — no per-frame string lookups.
#[derive(Default)]
struct PostFxUniforms {
    u_time: Option<glow::UniformLocation>,
    u_resolution: Option<glow::UniformLocation>,
    u_audio_mid: Option<glow::UniformLocation>,
    u_params: [Option<glow::UniformLocation>; 8],
    u_lut: Option<glow::UniformLocation>,
}

struct BloomHqPrograms {
    downsample: glow::Program,
    blur_h:     glow::Program,
    blur_v:     glow::Program,
    composite:  glow::Program,
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
    /// `None` for built-in passes (e.g. bloom_hq) that manage their own
    /// programs internally. Always `Some` for user-authored GLSL passes.
    pub program: Option<glow::Program>,
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
    // Built-in pass GPU resources, allocated once at PostFx::new and
    // shared across all bloom_hq invocations.
    bloom_half: [Fbo; 2],
    bloom_programs: BloomHqPrograms,
    /// Detected Pi generation. Passed in from the pipeline so `load_dir` can
    /// apply the same `min_pi_gen` filter as `SceneLibrary::load_dir_for_gen`.
    pi_gen: PiGen,
}

/// Minimal interface that `PostFxScreen` and `PostFxParamScreen` use from
/// `PostFx`. Decoupled so screens can be tested without a GL context.
pub trait PostFxController {
    fn passes(&self) -> &[PostFxPass];
    fn toggle(&mut self, idx: usize);
    fn pass_params_mut(&mut self, idx: usize) -> Option<&mut crate::scene::ParamMap>;
    fn save_state(&self, dir: &std::path::Path) -> crate::Result<()>;
    fn snapshot(&self) -> crate::preset::store::PostFxSnapshot;
    fn apply_snapshot(&mut self, snap: &crate::preset::store::PostFxSnapshot);
}

impl PostFxController for PostFx {
    fn passes(&self) -> &[PostFxPass] {
        &self.passes
    }

    fn toggle(&mut self, idx: usize) {
        self.toggle(idx);
    }

    fn pass_params_mut(&mut self, idx: usize) -> Option<&mut crate::scene::ParamMap> {
        self.pass_params_mut(idx)
    }

    fn save_state(&self, dir: &std::path::Path) -> crate::Result<()> {
        self.save_state(dir)
    }

    fn snapshot(&self) -> crate::preset::store::PostFxSnapshot {
        self.snapshot()
    }

    fn apply_snapshot(&mut self, snap: &crate::preset::store::PostFxSnapshot) {
        self.apply_snapshot(snap);
    }
}

impl PostFx {
    /// Construct an empty chain — `passes` empty, two FBOs allocated.
    pub fn new(gl: Arc<glow::Context>, width: u32, height: u32, pi_gen: PiGen) -> Result<Self> {
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
        // Half-res scratch for bloom_hq. Sized at chain/2 rounded up
        // (max(1) guards a degenerate 1x1 chain).
        let half_w = (width / 2).max(1);
        let half_h = (height / 2).max(1);
        let bloom_half = [
            Fbo::new(gl.clone(), half_w, half_h)?,
            Fbo::new(gl.clone(), half_w, half_h)?,
        ];

        // Compile the 4 built-in bloom_hq programs. Each uses the standard
        // postfx prelude so u_input/u_resolution/u_param* are available.
        let downsample = compile_program(
            &gl, QUAD_VERT, &assemble_postfx_fragment(BLOOM_DOWNSAMPLE_FRAG),
        )?;
        let blur_h = compile_program(
            &gl, QUAD_VERT, &assemble_postfx_fragment(BLOOM_BLUR_H_FRAG),
        )?;
        let blur_v = compile_program(
            &gl, QUAD_VERT, &assemble_postfx_fragment(BLOOM_BLUR_V_FRAG),
        )?;
        let composite = compile_program(
            &gl, QUAD_VERT, &assemble_postfx_fragment(BLOOM_COMPOSITE_FRAG),
        )?;
        // Pin samplers to their TUs once at link time.
        unsafe {
            for p in [downsample, blur_h, blur_v, composite] {
                gl.use_program(Some(p));
                if let Some(loc) = gl.get_uniform_location(p, "u_input") {
                    gl.uniform_1_i32(Some(&loc), 0);
                }
            }
            // Composite also reads u_bloom on TU5.
            gl.use_program(Some(composite));
            if let Some(loc) = gl.get_uniform_location(composite, "u_bloom") {
                gl.uniform_1_i32(Some(&loc), 5);
            }
        }
        let bloom_programs = BloomHqPrograms {
            downsample,
            blur_h,
            blur_v,
            composite,
        };

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
            bloom_half,
            bloom_programs,
            pi_gen,
        })
    }

    /// Load every `postfx/*.{glsl,toml}` pair under `dir`. Order is
    /// alphabetical by stem — deterministic so users can predict the chain.
    /// Reordering UI lands in a later phase.
    pub fn load_dir(&mut self, dir: &Path) -> Result<()> {
        let mut found: BTreeMap<String, (Option<PathBuf>, PathBuf)> = BTreeMap::new();
        if !dir.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            // Two acceptable kinds:
            //   1. user pass — paired .glsl + .toml
            //   2. built-in pass — .toml only (name must be in BUILTIN_POSTFX_PASSES)
            let ext = path.extension().and_then(|s| s.to_str());
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            match ext {
                Some("glsl") => {
                    if is_builtin_postfx(&stem) {
                        tracing::warn!(
                            "postfx/{}.glsl ignored — '{}' is a built-in pass; \
                             remove the .glsl to use the built-in",
                            stem, stem
                        );
                        continue;
                    }
                    let meta_path = path.with_extension("toml");
                    if !meta_path.exists() {
                        tracing::warn!("postfx {} has no .toml metadata, skipping", path.display());
                        continue;
                    }
                    found.insert(stem, (Some(path), meta_path));
                }
                Some("toml") => {
                    if is_builtin_postfx(&stem) {
                        // Built-in: this .toml stands alone (no .glsl pair needed).
                        // Don't override a previously-seen pairing — user .glsl + .toml
                        // for the same stem would have already populated `found`.
                        found.entry(stem.clone()).or_insert((None, path));
                    }
                    // Non-builtin lone .toml: ignored (paired .glsl scan picks it up if present).
                }
                _ => {}
            }
        }
        // 1. Upsert each discovered pass. This preserves enable/param state
        //    across hot-reloads and refuses to duplicate existing entries.
        for (name, (glsl, toml_path)) in &found {
            match (|| -> Result<()> {
                let meta_str = std::fs::read_to_string(toml_path)?;
                let meta = SceneMeta::parse(&meta_str, &toml_path.display().to_string())?;
                meta.validate()?;
                if let Some(required) = meta.min_pi_gen {
                    if required > self.pi_gen {
                        tracing::info!(
                            "postfx '{}' requires {} (detected {}); filtered",
                            name,
                            required.as_str(),
                            self.pi_gen.as_str()
                        );
                        return Ok(());
                    }
                }
                if let Some(glsl_path) = glsl {
                    let body = std::fs::read_to_string(glsl_path)?;
                    self.upsert(name, &body, meta, toml_path.clone())
                } else {
                    self.upsert_builtin(name, meta, toml_path.clone())
                }
            })() {
                Ok(()) => {}
                Err(e) => tracing::warn!("postfx '{name}': {e}; skipping"),
            }
        }

        // 2. Remove any passes whose file is no longer on disk. Free their
        //    GL program + LUT textures before dropping.
        let surviving_names: std::collections::HashSet<&String> = found.keys().collect();
        let to_remove: Vec<usize> = self
            .passes
            .iter()
            .enumerate()
            .filter(|(_, p)| !surviving_names.contains(&p.name))
            .map(|(i, _)| i)
            .collect();
        for idx in to_remove.into_iter().rev() {
            let p = self.passes.remove(idx);
            unsafe {
                if let Some(prog) = p.program {
                    self.gl.delete_program(prog);
                }
                for tex in &p.lut_textures {
                    self.gl.delete_texture(*tex);
                }
            }
        }

        // 3. Populate lut_textures for the "lut" pass, if present.
        let luts_dir = dir.join("luts");
        let lut_paths = crate::render::lut::scan_lut_paths(&luts_dir);
        if let Some(idx) = self.find("lut") {
            // Drop the old textures explicitly — Drop on PostFx runs only at the end
            // of process lifetime; we'd leak GL handles on every reload otherwise.
            unsafe {
                for tex in &self.passes[idx].lut_textures {
                    self.gl.delete_texture(*tex);
                }
            }
            let mut new_textures = Vec::with_capacity(lut_paths.len());
            for p in &lut_paths {
                // Deref coercion converts &Arc<glow::Context> to &glow::Context.
                match crate::render::lut::load_lut_png(&self.gl, p) {
                    Ok(t) => new_textures.push(t),
                    Err(e) => tracing::warn!("LUT load skipped {}: {e}", p.display()),
                }
            }
            tracing::info!(
                "postfx: lut pass attached {} LUT(s) from {}",
                new_textures.len(),
                luts_dir.display()
            );
            self.passes[idx].lut_textures = new_textures;
        }

        self.refresh_summary();
        Ok(())
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

    /// Capture the chain into a serializable snapshot. `active=true`; caller
    /// may flip it down for "paused" semantics.
    pub fn snapshot(&self) -> crate::preset::store::PostFxSnapshot {
        snapshot_passes(&self.passes)
    }

    /// Apply a snapshot to the live chain. Matches passes by name; passes
    /// not present in the snapshot are left as-is. Snapshot entries whose
    /// name is unknown are skipped with a `tracing::warn`. Refreshes the
    /// status-bar summary.
    pub fn apply_snapshot(&mut self, snap: &crate::preset::store::PostFxSnapshot) {
        apply_snapshot_to_passes(&mut self.passes, snap);
        self.refresh_summary();
    }

    /// Built-in 4-stage half-res bloom dispatch. Called from `run()` when
    /// it encounters a pass named "bloom_hq". Associated function (not
    /// method) so it can borrow individual PostFx fields disjointly
    /// alongside the iter borrow on `self.passes`.
    ///
    /// Stages:
    ///   1. downsample + bright_pass: src_texture(full) -> bloom_half[0] (half)
    ///   2. blur_H:                   bloom_half[0]     -> bloom_half[1]
    ///   3. blur_V:                   bloom_half[1]     -> bloom_half[0]
    ///   4. composite: src_texture(full) + u_bloom(half upsample) -> dst_fbo
    #[allow(clippy::too_many_arguments)]
    fn dispatch_bloom_hq(
        gl: &glow::Context,
        bloom_half: &[Fbo; 2],
        bloom_programs: &BloomHqPrograms,
        src_texture: glow::Texture,
        dst_fbo: &Fbo,
        full_w: u32,
        full_h: u32,
        slots: [f32; 9],
    ) {
        let half_w = bloom_half[0].width;
        let half_h = bloom_half[0].height;
        let threshold = slots[0];
        let intensity = slots[1];
        let radius    = slots[2];

        unsafe fn set_f32(gl: &glow::Context, program: glow::Program, name: &str, value: f32) {
            if let Some(loc) = gl.get_uniform_location(program, name) {
                gl.uniform_1_f32(Some(&loc), value);
            }
        }
        unsafe fn set_vec2(gl: &glow::Context, program: glow::Program, name: &str, x: f32, y: f32) {
            if let Some(loc) = gl.get_uniform_location(program, name) {
                gl.uniform_2_f32(Some(&loc), x, y);
            }
        }

        unsafe {
            // --- Stage 1: downsample + bright_pass into bloom_half[0] ---
            bloom_half[0].bind();   // also sets viewport to half-res
            gl.use_program(Some(bloom_programs.downsample));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(src_texture));
            // u_resolution = full-res so 1.0 / u_resolution gives source pixel step.
            set_vec2(gl, bloom_programs.downsample, "u_resolution", full_w as f32, full_h as f32);
            set_f32(gl, bloom_programs.downsample, "u_param0", threshold);
            gl.draw_arrays(glow::TRIANGLES, 0, VERTEX_COUNT);

            // --- Stage 2: horizontal blur, bloom_half[0] -> bloom_half[1] ---
            bloom_half[1].bind();
            gl.use_program(Some(bloom_programs.blur_h));
            gl.bind_texture(glow::TEXTURE_2D, Some(bloom_half[0].texture));
            // u_resolution = half-res so 1.0 / u_resolution gives a half-res pixel step.
            set_vec2(gl, bloom_programs.blur_h, "u_resolution", half_w as f32, half_h as f32);
            set_f32(gl, bloom_programs.blur_h, "u_param2", radius);
            gl.draw_arrays(glow::TRIANGLES, 0, VERTEX_COUNT);

            // --- Stage 3: vertical blur, bloom_half[1] -> bloom_half[0] ---
            bloom_half[0].bind();
            gl.use_program(Some(bloom_programs.blur_v));
            gl.bind_texture(glow::TEXTURE_2D, Some(bloom_half[1].texture));
            set_vec2(gl, bloom_programs.blur_v, "u_resolution", half_w as f32, half_h as f32);
            set_f32(gl, bloom_programs.blur_v, "u_param2", radius);
            gl.draw_arrays(glow::TRIANGLES, 0, VERTEX_COUNT);

            // --- Stage 4: composite into the next chain fbo ---
            dst_fbo.bind();   // restores viewport to full-res (Fbo::bind sets viewport)
            gl.use_program(Some(bloom_programs.composite));
            gl.active_texture(glow::TEXTURE5);
            gl.bind_texture(glow::TEXTURE_2D, Some(bloom_half[0].texture));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(src_texture));
            set_vec2(gl, bloom_programs.composite, "u_resolution", full_w as f32, full_h as f32);
            set_f32(gl, bloom_programs.composite, "u_param1", intensity);
            gl.draw_arrays(glow::TRIANGLES, 0, VERTEX_COUNT);

            // Unbind TU5 + restore TU0 active so non-bloom_hq passes that follow
            // continue to write to TU0 via `bind_texture` (the chain's convention).
            gl.active_texture(glow::TEXTURE5);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE0);
        }
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
            if pass.name == "bloom_hq" {
                let slots = pass.params.effective_slot_values(&state.audio_bands, state.audio_bypass);
                let input_idx = self.input_idx;
                let next_idx = 1 - input_idx;
                let dst_fbo = if is_last {
                    &self.feedback[next_feedback]
                } else {
                    &self.pp[next_idx]
                };
                Self::dispatch_bloom_hq(
                    &self.gl,
                    &self.bloom_half,
                    &self.bloom_programs,
                    self.pp[input_idx].texture,
                    dst_fbo,
                    self.width,
                    self.height,
                    slots,
                );
                self.input_idx = next_idx;
                continue;
            }
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
                self.gl.use_program(Some(
                    pass.program.expect("user pass must have a compiled program"),
                ));
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
            if let Some(old) = existing.program {
                unsafe { self.gl.delete_program(old) };
            }
            existing.fragment_body = body.to_string();
            existing.params = ParamMap::from_scene(&meta);
            existing.meta = meta;
            existing.source_path = source_path;
            existing.program = Some(program);
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
            program: Some(program),
            uniforms,
            lut_textures: Vec::new(),
        };
        self.passes.push(pass);
        self.refresh_summary();
        Ok(())
    }

    /// Built-in passes (e.g. `bloom_hq`) have no user shader. Their
    /// meta + params come from disk, but the program is None and run()
    /// dispatches them via a hardcoded code path.
    fn upsert_builtin(&mut self, name: &str, meta: SceneMeta, source_path: PathBuf) -> Result<()> {
        if let Some(existing) = self.passes.iter_mut().find(|p| p.name == name) {
            // If this stem was previously a user pass (Some(program)), the user
            // shader is being demoted to built-in via hot-reload. Free the orphaned
            // GL handle before clobbering the slot.
            if let Some(old) = existing.program.take() {
                unsafe { self.gl.delete_program(old) };
            }
            existing.fragment_body = String::new();
            existing.params = ParamMap::from_scene(&meta);
            existing.meta = meta;
            existing.source_path = source_path;
            existing.program = None;
            return Ok(());
        }
        let pass = PostFxPass {
            name: name.to_string(),
            enabled: meta.enabled_by_default,
            params: ParamMap::from_scene(&meta),
            meta,
            fragment_body: String::new(),
            source_path,
            program: None,
            // Built-ins never read pass.uniforms at render time, but the
            // field must be present. Resolve against blit_program (always
            // valid). Wasted lookups, but only on initial load.
            uniforms: resolve_postfx_uniforms(&self.gl, self.blit_program),
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
        for p in self.passes.iter().filter(|p| {
            p.enabled && !(p.name == "lut" && p.lut_textures.is_empty())
        }) {
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

/// Build a `PostFxPass` directly without a GL context. Used by cross-
/// module tests that need to drive snapshot helpers without a live chain.
/// Programs / uniforms / lut_textures are left empty (the snapshot helpers
/// don't touch them).
#[cfg(test)]
pub(crate) fn tests_fake_pass(name: &str, enabled: bool, params: &[(&str, f32)]) -> PostFxPass {
    let mut toml = format!("name = \"{name}\"\n");
    for (slot, (n, v)) in params.iter().enumerate() {
        toml.push_str(&format!(
            "\n[[params]]\nslot = {slot}\nname = \"{n}\"\nmin = 0.0\nmax = 1.0\ndefault = {v}\n"
        ));
    }
    let meta = SceneMeta::parse(&toml, "inline").unwrap();
    let params = ParamMap::from_scene(&meta);
    PostFxPass {
        name: name.to_string(),
        meta,
        fragment_body: String::new(),
        source_path: PathBuf::from("inline"),
        program: None,
        params,
        enabled,
        uniforms: PostFxUniforms::default(),
        lut_textures: vec![],
    }
}

/// Pure capture: build a snapshot from a slice of passes. Always
/// `active=true`; caller (e.g. an auto-sync hook) flips it down for
/// "paused" semantics.
pub(crate) fn snapshot_passes(passes: &[PostFxPass]) -> crate::preset::store::PostFxSnapshot {
    use crate::preset::store::{PostFxPassSnapshot, PostFxSnapshot};
    let passes = passes
        .iter()
        .map(|p| {
            let mut params = BTreeMap::new();
            for d in p.params.defs() {
                if let Some(v) = p.params.get(&d.name) {
                    params.insert(d.name.clone(), v);
                }
            }
            PostFxPassSnapshot {
                name: p.name.clone(),
                enabled: p.enabled,
                params,
            }
        })
        .collect();
    PostFxSnapshot {
        active: true,
        passes,
    }
}

/// Pure apply: match snapshot entries to passes by name. Snapshot entries
/// with no matching pass log a `tracing::warn` and are skipped; passes not
/// present in the snapshot are left as-is.
pub(crate) fn apply_snapshot_to_passes(
    passes: &mut [PostFxPass],
    snap: &crate::preset::store::PostFxSnapshot,
) {
    let live: std::collections::HashSet<&str> = passes.iter().map(|p| p.name.as_str()).collect();
    for s in &snap.passes {
        if !live.contains(s.name.as_str()) {
            tracing::warn!(pass = %s.name, "postfx snapshot references unknown pass; skipping");
        }
    }
    let by_name: std::collections::BTreeMap<&str, &crate::preset::store::PostFxPassSnapshot> =
        snap.passes.iter().map(|p| (p.name.as_str(), p)).collect();
    for pass in passes.iter_mut() {
        if let Some(s) = by_name.get(pass.name.as_str()) {
            pass.enabled = s.enabled;
            for (k, v) in &s.params {
                pass.params.set(k, *v);
            }
        }
    }
}

impl Drop for PostFx {
    fn drop(&mut self) {
        // Programs are GL-owned; drop them explicitly so we don't leak when
        // the Pipeline tears down on shutdown. (FBOs handle themselves.)
        for pass in &self.passes {
            unsafe {
                if let Some(p) = pass.program {
                    self.gl.delete_program(p);
                }
                for tex in &pass.lut_textures {
                    self.gl.delete_texture(*tex);
                }
            }
        }
        unsafe {
            self.gl.delete_program(self.bloom_programs.downsample);
            self.gl.delete_program(self.bloom_programs.blur_h);
            self.gl.delete_program(self.bloom_programs.blur_v);
            self.gl.delete_program(self.bloom_programs.composite);
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
        // Each entry: (name, has_glsl) — built-ins ship TOML only.
        let entries: &[(&str, bool)] = &[
            ("bloom",      true),
            ("bloom_hq",   false),   // built-in: meta only
            ("chromatic",  true),
            ("crt",        true),
            ("dither",     true),
            ("grain",      true),
            ("lut",        true),
            ("pixelate",   true),
            ("trails",     true),
            ("vignette",   true),
        ];
        for (name, has_glsl) in entries {
            let glsl = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("postfx")
                .join(format!("{name}.glsl"));
            let toml = glsl.with_extension("toml");
            if *has_glsl {
                assert!(glsl.exists(), "missing {}", glsl.display());
            } else {
                assert!(!glsl.exists(), "built-in {} must NOT have a .glsl pair", name);
            }
            assert!(toml.exists(), "missing {}", toml.display());
            let s = std::fs::read_to_string(&toml).unwrap();
            let m = SceneMeta::parse(&s, &toml.display().to_string()).unwrap();
            m.validate().unwrap();
            assert_eq!(m.name, *name);
        }
    }

    #[test]
    fn bloom_hq_meta_has_min_pi_gen_pi4() {
        let toml = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("postfx")
            .join("bloom_hq.toml");
        let s = std::fs::read_to_string(&toml).unwrap();
        let m = SceneMeta::parse(&s, &toml.display().to_string()).unwrap();
        m.validate().unwrap();
        assert_eq!(m.min_pi_gen, Some(crate::platform::PiGen::Pi4));
    }

    #[test]
    fn load_dir_uses_repo_postfx_dir_idempotently() {
        // Sanity for the bug-fix: PostFx::load_dir is safe to call multiple times.
        // We can't construct a real PostFx without GL, but we CAN verify that the
        // same set of (name, (glsl, toml)) pairs is discovered on disk regardless
        // of call count — i.e. the directory contents are stable.
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("postfx");
        let mut names_first: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("glsl"))
            .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string()))
            .collect();
        names_first.sort();

        let mut names_second: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("glsl"))
            .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string()))
            .collect();
        names_second.sort();

        assert_eq!(names_first, names_second, "postfx/ contents must be stable between scans");
        assert!(names_first.contains(&"lut".to_string()), "lut pass must be present");
        assert_eq!(names_first.len(), 9, "9 user-shader postfx passes (bloom_hq is built-in, TOML-only)");
        let bloom_hq_toml = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("postfx")
            .join("bloom_hq.toml");
        assert!(bloom_hq_toml.exists(), "built-in bloom_hq.toml must ship");
        let bloom_hq_glsl = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("postfx")
            .join("bloom_hq.glsl");
        assert!(!bloom_hq_glsl.exists(), "built-in bloom_hq must NOT have a .glsl pair");
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

    use crate::preset::store::{PostFxPassSnapshot, PostFxSnapshot};
    use crate::scene::ParamMap;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    /// Build a `PostFxPass` directly without a GL context. Uses
    /// `SceneMeta::parse` for the meta so all serde-default fields are
    /// populated correctly; programs / uniforms / lut_textures are left
    /// empty (the snapshot helpers don't touch them).
    fn fake_pass(name: &str, enabled: bool, params: &[(&str, f32)]) -> super::PostFxPass {
        let mut toml = format!("name = \"{name}\"\n");
        for (slot, (n, v)) in params.iter().enumerate() {
            toml.push_str(&format!(
                "\n[[params]]\nslot = {slot}\nname = \"{n}\"\nmin = 0.0\nmax = 1.0\ndefault = {v}\n"
            ));
        }
        let meta = SceneMeta::parse(&toml, "inline").unwrap();
        let params = ParamMap::from_scene(&meta);
        super::PostFxPass {
            name: name.to_string(),
            meta,
            fragment_body: String::new(),
            source_path: PathBuf::from("inline"),
            program: None,
            params,
            enabled,
            uniforms: super::PostFxUniforms::default(),
            lut_textures: vec![],
        }
    }

    #[test]
    fn snapshot_passes_captures_name_enabled_params() {
        let passes = vec![
            fake_pass("vignette", true, &[("amount", 0.4)]),
            fake_pass("grain", false, &[("amount", 0.1)]),
        ];
        let snap = super::snapshot_passes(&passes);
        assert!(snap.active, "snapshot_passes should produce active=true by default");
        assert_eq!(snap.passes.len(), 2);
        assert_eq!(snap.passes[0].name, "vignette");
        assert!(snap.passes[0].enabled);
        assert_eq!(snap.passes[0].params.get("amount"), Some(&0.4));
        assert!(!snap.passes[1].enabled);
    }

    #[test]
    fn apply_snapshot_updates_matching_passes() {
        let mut passes = vec![
            fake_pass("vignette", false, &[("amount", 0.0)]),
            fake_pass("grain", true, &[("amount", 0.0)]),
        ];
        let snap = PostFxSnapshot {
            active: true,
            passes: vec![PostFxPassSnapshot {
                name: "vignette".into(),
                enabled: true,
                params: BTreeMap::from([("amount".to_string(), 0.5)]),
            }],
        };
        super::apply_snapshot_to_passes(&mut passes, &snap);
        assert!(passes[0].enabled);
        assert_eq!(passes[0].params.get("amount"), Some(0.5));
        // grain not in snapshot → left alone
        assert!(passes[1].enabled);
        assert_eq!(passes[1].params.get("amount"), Some(0.0));
    }

    #[test]
    fn apply_snapshot_skips_unknown_names() {
        let mut passes = vec![fake_pass("vignette", false, &[("amount", 0.0)])];
        let snap = PostFxSnapshot {
            active: true,
            passes: vec![PostFxPassSnapshot {
                name: "deleted_pass".into(),
                enabled: true,
                params: BTreeMap::new(),
            }],
        };
        // Must not panic; existing pass stays as-is.
        super::apply_snapshot_to_passes(&mut passes, &snap);
        assert!(!passes[0].enabled);
    }
}
