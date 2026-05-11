# mandleROT Roadmap Specs

Full feature specifications. One section per numbered item in
`ROADMAP.md::Execution Order`. The roadmap index stays compact; details
live here.

---

## 2. Double-tap Esc → Panic from inside menu

**Problem.** Menu screens swallow `Esc` (pop). With a menu open the user
has no way to fire `Action::Panic` without first closing the menu manually.

**Design.**

- A 400ms double-tap detector at the top of the main-loop input dispatch.
- Track `last_esc_at: Option<Instant>` next to `ui_stack` in `main.rs`.
- When an `Esc` (or `Backspace`) is observed:
  - If `last_esc_at` set within 400ms → fire `Action::Panic` *and* call
    `ui_stack.close_all()`. Clear `last_esc_at`.
  - Else stamp `last_esc_at` and route to stack-first / keymap as today.
- Single Esc still pops one screen (menu open) or fires Panic (menu closed)
  — that path is unchanged.

**Test plan.** Two synthetic Esc events 200ms apart with menu open →
state ends up at SAFE_SCENE, stack empty. Two events 600ms apart with menu
open → first pops, second pops again (or panic if stack empty).

---

## 3. Active-layer: solid inverted half

**Problem.** With both layer headers + param rows side-by-side, the
selected layer is only signalled by the `>` cursor in one of two columns.
Easy to miss at a glance.

**Design.**

- After `state_to_grid` paints normally, walk the active layer's column
  range and OR `ATTR_INVERSE` into every cell.
- Active = `Layer::A` → cols `1..=39` rows `1..=10` (header + 8 param rows
  + xfade label row stops at 9). Skip the vertical separator at col 40.
- Active = `Layer::B` → cols `41..=78`.
- Keep the `>` cursor logic; inverted-on-inverted reads as a normal
  bright `>` on amber.

**Test plan.** Render state with `active_layer = A` and assert col 5 row 2
has `ATTR_INVERSE` bit set; col 45 row 2 does not. Flip and re-assert.

---

## 4. F4 → SettingsScreen root

**Problem.** F4 currently jumps straight to SLOTS. Future settings
(audio, preferences, key rebinder) need a home — a flat top-level menu
doesn't scale.

**Design.**

- New `src/ui/screens/settings.rs` with `SettingsScreen` listing three
  entries:
  1. Preferences (stub: "Coming soon")
  2. Audio
  3. Slot Mapper
- Enter on an entry → `ScreenResult::Push(...)`.
- `MenuKind::Settings` replaces `MenuKind::Slots` as the F4 target.
  `MenuKind::Slots` removed; entry into SLOTS only via the settings root.
- Selection cursor + arrow keys + 1/2/3 digit shortcuts.

**File changes.** `src/action.rs`, `keymap.toml`, `src/main.rs` (open
`SettingsScreen` instead of `SlotsScreen`), `src/ui/screens/{mod,settings}.rs`.

---

## 5. AudioSettingsScreen + live-tunable audio params

**Problem.** Noise floor is currently env-var-only (`MANDLEROT_NOISE_FLOOR`).
Per-band gain has no knob at all. Tuning requires editing the unit file +
restart — useless mid-show.

**Design.**

- New `AudioParams` shared struct (Arc-of-AtomicF32 for each field) owned
  by main, cloned into the audio thread on spawn:
  ```rust
  struct AudioParams {
      noise_floor: AtomicF32,
      gain_bass: AtomicF32,
      gain_lomid: AtomicF32,
      gain_himid: AtomicF32,
      gain_treble: AtomicF32,
  }
  ```
- `AudioGain::normalize` reads `noise_floor` from the shared struct each
  call (current field becomes `Arc<AudioParams>`).
- Per-band gain multiplied onto the post-normalize value before push to
  `AtomicAudio`.
- `AudioSettingsScreen` renders 5 horizontal sliders. ↑↓ select knob, ←→
  adjust. `r` resets the focused knob to its default. Persist to
  `<state_dir>/audio.toml` on every change.

**File changes.** `src/audio/{envelope,thread,params}.rs`, `src/ui/screens/audio.rs`.

**Persistence layout.**
```toml
noise_floor = 8.0
gain_bass = 1.0
gain_lomid = 1.0
gain_himid = 1.0
gain_treble = 1.0
```

---

## 6. Rename Preset → Look

**Problem.** "Preset" reads as per-layer in VJ idiom, but our `Preset`
already holds both scenes + xfade + blend + params — a full combined
state. Misleading.

**Design.**

- `Mode::Preset` → `Mode::Look`.
- `PresetStore` → `LookStore`; `Preset` → `Look`; `PresetFile` → `LooksFile`.
- `presets.json` → `looks.json`. On first load: if `looks.json` missing
  but `presets.json` exists, rename in place (one-time migration log).
- Status panel: "PST" → "LK", "PRESETS" header → "LOOKS".
- CLI: `--presets` → `--looks` (keep `--presets` as deprecated alias for
  one release).
- README + design doc references.

**File changes.** broad but mechanical: `state.rs`, `apply.rs`,
`preset/store.rs` → rename file + types, `status/compose.rs`,
`status/overlay.rs`, `main.rs`, `keymap.toml` comments, README.

---

## 7. Scene: CRT signal collapse

**Problem.** No "broken hardware" aesthetic in the scene library.

**Design.**

- Base layer: gradient or color bars derived from `u_audio` (auto-stays
  visually loud).
- Effects pipeline (all in fragment shader):
  - **Scanlines**: `sin(uv.y * resolution.y * pi)` modulation, 70% depth.
  - **Chromatic aberration**: sample R/G/B at `uv ± k`. `k` scales with
    `u_audio.treble`.
  - **Vertical-hold roll**: triggered by `u_beat`. After a beat, for next
    ~200ms, shift `uv.y` by `fract(u_time * 3.0)`. Reads as a screen
    rolling.
  - **Static snow** band at random y rows (hash-gated, treble-driven).

**Params (slots 0..7).** `scan_depth`, `aberration`, `roll_rate`, `snow_amount`, `hue`, `brightness`, `roll_speed`, `tint_warp`.

---

## 8. Scene: VHS tracking

**Design.**

- Chroma offset: sample Y at `uv`, U/V at `uv + vec2(0.01, 0)`. Bleeds
  color right of luma.
- **Head-switch noise band**: a single 4px-tall band of TV-snow that
  rolls upward at ~0.3px/frame. Use `mod(u_time * speed, 1.0)` for y.
- **Dropout tears**: at random y rows (hash-gated by time), shift uv.x
  hard by ±0.1 for a single scanline.
- **Color bleed**: blur U/V channels horizontally with a 5-tap kernel.
- Audio: treble drives dropout count, bass drives chroma offset width.

**Params.** `chroma_offset`, `head_band_y_rate`, `dropout_rate`, `bleed`, `hue_shift`, `saturation`, `desync`, `brightness`.

---

## 9. Scene: Datamosh blocks

**Design.**

- Sample `u_prev` (previous frame texture) as the base "I-frame".
- Per-16x16-block motion vector computed from audio band derivative
  (bass = horizontal, lomid = vertical). Each block's `uv` is offset by
  its motion vector before sampling u_prev.
- Beat = forced keyframe reset (zero motion that frame; allows the
  visual to re-stabilize, then drift again).
- Optional: per-block hue-shift driven by hash(block_idx, time_bucket).

**Params.** `block_size`, `motion_bass`, `motion_lomid`, `mosh_intensity`, `hue_drift`, `keyframe_decay`, `chroma_corruption`, `noise_floor`.

---

## 10. Scene: Hex/binary rain

**Design.**

- Reuse `ascii_rain` skeleton (column hash, per-cell sub-pixel glyph,
  head/trail).
- Glyph set: 16 chars (0..9, A..F). Sub-pixel bitmaps are 5x7 like the
  Matrix glyphs.
- Slower fall, denser columns (~50% active vs. ASCII's ~25%), tighter
  trail to read as "data console" not "rain".
- Palette: amber (BIOS) by default, green via `palette_shift` param.

**Params.** `speed`, `glyph_rate`, `density`, `trail_decay`, `palette_shift` (0=amber, 1=green, blends), `head_brightness`, `noise_amount`, `binary_mix` (0=hex, 1=binary, blends).

---

## 24. Composite video input

**Problem.** mandleROT renders entirely synthetic visuals. A *very* on-brand
feature for a VJ tool: feed in a live external source (security cam, VHS
deck, broadcast) as one of the two layers, then blend / glitch it through
the existing pipeline.

**Hardware path.**
- The Pi 4/5 composite jack is **output-only** — no native composite-in.
- Two viable capture paths, both expose a `/dev/videoN` v4l2 device:
  1. **USB composite-to-video capture dongle** (e.g. EasyCap UVC clones,
     ~$15). Plug analog signal → USB → v4l2. Latency is typically 100-200ms
     and resolution caps around 720x480.
  2. **HDMI-to-USB capture** (e.g. MS2109 chipset dongles, ~$20). Better
     latency + 720p, but requires the source to have HDMI out.
- Either way, the rust side is a v4l2 → texture-upload loop.

**Rust design.**
- New `src/video/` module with:
  - `Capture` struct wrapping the `v4l` crate (Linux-only, behind a feature
    flag). Spawns a worker thread that pulls frames into a triple buffer.
  - `VideoFrame { width, height, rgba: Vec<u8>, ts: Instant }`. Capture
    thread converts YUYV/MJPEG → RGBA8 once; render thread sees only RGBA.
- New uniform `sampler2D u_video` exposed by the prelude. Empty texture
  (1x1 black) when no capture device is present.
- New layer scene type: `__video__` is a baked-in scene whose body is just
  `gl_FragColor = texture2D(u_video, v_uv);`. Selecting `__video__` on any
  layer pipes live capture into that layer; xfade + blend modes work
  normally on top.
- Status panel: a `VID:active`/`VID:--` chip in the top bar.

**Config.**
```toml
[video]
device = "/dev/video0"   # falls back to first found if missing
width  = 720
height = 480
mode   = "yuyv"          # or "mjpeg" / "auto"
```

**Open questions.**
- Where to do YUYV → RGB conversion: CPU in the capture thread (simple,
  works on any GL) or GLES shader sampling YUYV directly (zero-copy upload
  but more GL plumbing)? Default = CPU, simpler.
- Latency tolerance: capture is async, may be 1-3 frames behind. Acceptable
  for VJ use? (Almost certainly yes — VJ is forgiving.)
- macOS dev path: no `/dev/video*`. Use `nokhwa` cross-platform crate, or
  stub video on non-Linux with a test pattern? Default = test pattern stub
  so the rest of the pipeline still demos on the dev box.

**Test plan.**
- Unit: `Capture::open` returns `Err` cleanly when device missing.
- Manual: plug a dongle into a Pi, point at a phone playing video, verify
  the layer shows the live feed within ~250ms.

---

## 25. Additional blend modes

**Problem.** We ship 5 blend modes (Mix, Add, Multiply, Screen, Difference).
Standard VJ kit usually offers 10-15. The blend dispatch in `shaders/blend.glsl`
is structured to make additions cheap.

**Design.**
- Extend `BlendMode` enum with new variants. Each maps to an integer that
  shader code dispatches on (the same pattern as today's 0..=4).
- New modes (Photoshop names, all per-channel unless noted):
  - **Overlay** — `2*a*b` when b<0.5 else `1-2*(1-a)*(1-b)`. Contrast boost.
  - **Soft Light** — gentler overlay.
  - **Hard Light** — overlay with A and B swapped.
  - **Color Dodge** — `b / (1-a)` clamped. Brightens.
  - **Color Burn** — `1 - (1-b)/a` clamped. Darkens.
  - **Linear Dodge (Add)** — already shipped as Add.
  - **Linear Burn** — `a + b - 1` clamped.
  - **Lighten** — `max(a, b)` per channel.
  - **Darken** — `min(a, b)` per channel.
  - **Exclusion** — `a + b - 2*a*b`. Softer Difference.
  - **Subtract** — `b - a` clamped.
  - **HSL family** — replace one component of B's HSL with A's:
    - **Hue** (A's hue, B's S+L)
    - **Saturation** (A's S, B's H+L)
    - **Color** (A's H+S, B's L)
    - **Luminosity** (A's L, B's H+S)

**File changes.**
- `src/state.rs::BlendMode` — add variants; renumber the discriminants
  carefully (existing on-disk Looks reference the integer mode).
- `BlendMode::parse` — add the new string keys.
- `BlendMode::as_int` — auto via discriminant.
- `shaders/blend.glsl` — add an `else if (u_blend_mode == N) { ... }` arm
  per mode. HSL modes need a tiny `rgb2hsl`/`hsl2rgb` helper.
- `src/apply.rs::BLEND_MODES` — append.
- Status panel + overlay: shorten the longer names (e.g. "ColDodge", "HardLt")
  to fit the 6-char field.

**Migration.** Existing looks store the blend by *string*, so renumbering
discriminants only affects the `as_int()` shader path — safe.

**Test plan.**
- Unit: each new mode parses from its canonical string + a short alias.
- Unit: shader test (golden image diff) for at least Overlay + Soft Light
  + one HSL mode — cover the math and the HSL helper.

---

## 26. Post-processing FX pipeline

**Problem.** Some effects belong at the *composite* layer (after A and B
are blended), not inside a single scene: bloom, vignette, grain, global
chromatic aberration, LUT color grading, pixelate, output-stage CRT
overlay. Currently the blend output goes straight to the swapchain — no
hook for these.

**Design.**

```
[Scene A] ─┐
           ├→ Blend → [PostFX 1] → [PostFX 2] → ... → swapchain / composite-out
[Scene B] ─┘
```

- A new `PostFx` struct holds an ordered list of `PostFxPass`. Each pass
  owns one fragment shader and a render target (FBO + texture). The chain
  pingpongs between two FBOs so any pass can sample the previous one.
- Each pass is configured by a TOML stanza identical in shape to a scene
  (`scenes/postfx_*.{glsl,toml}`?) or a dedicated `postfx/` dir — leaning
  toward `postfx/` since these aren't selectable as layers.
- A pass is `enabled: bool` + 8 params, just like scenes. The PostFX
  settings screen (item 5's sibling) lets the user toggle, reorder, tune.

**Initial passes (ship with v1 of this feature).**
- **Bloom** — bright-pass + 5-tap Gaussian blur + add back. Two FBOs.
- **Vignette** — radial darkening at edges.
- **Film grain** — per-pixel hashed noise modulated by luma.
- **Chromatic aberration** — global RGB split, distance-from-center scaled.
- **Pixelate / downsample** — chunky pixel look at variable cell size.
- **LUT color grade** — sample a 16x16x16 LUT (encoded as a 256x16 RGBA
  PNG, the standard format). LUT files live in `postfx/luts/*.png`.
- **CRT overlay** — scanlines + curvature + slot-mask. Different from the
  per-scene `phosphor_crt` because it goes over *everything* including
  the safe-scene fallback.
- **Dither** — Bayer 4x4 quantize to N steps (output-stage 1-bit option).

**File changes.**
- `src/render/postfx.rs` — new module with `PostFx`, `PostFxPass`, FBO
  ping-pong management.
- `src/render/pipeline.rs` — wire `postfx.run(blend_output) → final_fbo`
  into the frame loop.
- `src/render/fbo.rs` — already exists; may need a second FBO for ping-pong.
- `shaders/postfx_*.glsl` — one per pass (or one dispatch shader with
  uniforms, TBD).
- `src/ui/screens/postfx.rs` — list of passes, toggle + reorder + per-pass
  param drawer (push another screen for tuning).
- New `Action::PostFxToggle { idx }` + `Action::PostFxParam { idx, slot, dir }`.
- `<state_dir>/postfx.toml` — persist enabled passes + their params.

**Performance budget.** Pi 4 at 720x480 has plenty of GPU headroom for
3-4 fullscreen passes; >6 stacked starts to push frame budget. Default
config ships only Vignette + Grain enabled — light and broadly flattering.

**Open questions.**
- Should PostFX be per-Look (saved with the look) or global? Default =
  global, with an explicit "save with look" toggle.
- LUT format: 16x16x16 strip vs. 16x16 atlas? Standard tools (Davinci,
  Premiere) export both; pick one. Default = 256x16 strip.

**Test plan.**
- Unit: PostFx chain with 0 passes is a no-op (output == input).
- Unit: chain with 1 pass writes to the second FBO, returns its texture id.
- Integration: render a frame through Bloom + Vignette + Grain, assert no
  GL errors and that grain produces per-pixel variance.

---

## 27. Chromakey output mode

**Problem.** The default workflow is "mandleROT *is* the visual output."
For multi-source VJ setups you usually want a hardware video mixer in the
chain (V-4EX, ATEM Mini, TriCaster, even a cheap HDMI matrix with key
support). The mixer needs mandleROT to emit a known *key color* (typically
chroma green `#00FF00` or magenta `#FF00FF`) anywhere the scene's "no
content" pixels live, so it can punch a clean hole and composite over a
camera feed / VJ deck output.

This is fundamentally different from PostFX bloom/grain etc. — those are
*creative* passes. Chromakey is an *output* mode that fundamentally
changes what the composite signal *means* downstream.

**Design.**

Three layers, smallest → largest commitment:

1. **Per-scene `keyable` flag in scene meta TOML.**
   ```toml
   name = "ascii_rain"
   keyable = true
   keyable_luma = 0.04   # luma below this is treated as "background"
   ```
   - `keyable = true` (default `false`) marks a scene as having a
     discardable background. The scene library reads this; nothing changes
     at scene-render time.
   - Audit pass: ascii_rain, sparkle, donut, boids, vectorscope, speed_lines,
     hex_rain, transform_rings — all naturally black-background = mark `true`.
     Fractals, plasma, voxel_terrain, tunnel — fill the screen = leave `false`.

2. **Chromakey post-pass.** Runs as the final stage *after* PostFX (or
   *before*, see Open Questions). One full-screen shader:
   ```glsl
   vec4 src = texture2D(u_blend, v_uv);
   float l   = dot(src.rgb, vec3(0.299, 0.587, 0.114));
   float key = step(l, u_key_luma);  // 1 = background
   vec3 col  = mix(src.rgb, u_key_color, key);
   gl_FragColor = vec4(col, 1.0);
   ```
   - When *both* layers are `keyable`, the key color punches through any
     pixel both layers agree is background. When only one is keyable, the
     opaque one wins and the key never activates. This emerges naturally
     from doing the keying *after* blend.
   - Soft edge: replace `step` with `smoothstep(u_key_luma - 0.02, u_key_luma + 0.02, l)` and `mix` with the same factor.

3. **Chromakey screen** under Settings → Chromakey:
   - Toggle on/off
   - Key color picker (presets: green / magenta / blue / custom)
   - Luma threshold knob (0.0 .. 0.15)
   - Edge softness knob
   - "Spill suppression" toggle: subtract the key color's chroma component
     from non-keyed pixels so text edges don't have green halos
   - State persisted to `<state_dir>/chromakey.toml`

**State + plumbing.**
- New `ChromakeyState { enabled, key_color: [f32;3], luma_threshold, edge_soft, spill_suppress }` on SharedState.
- New `Action::ChromakeyToggle` bound to a key (proposal: `K`).
- Overlay strip shows `KEY:G` / `KEY:M` / `KEY:--` chip when enabled, so
  the operator can't forget the mode is on.
- Status panel safe-scene fallback: when chromakey is active, the SMPTE
  bars should also key — except the bars are entirely opaque so the user
  sees the fallback intact, which is the right safety behavior.

**Audit table (initial `keyable` defaults).**

| Scene | keyable | Reason |
|---|---|---|
| ascii_rain, hex_rain | ✅ | Dark cells between glyphs |
| sparkle, transform_rings, speed_lines | ✅ | Designed against black |
| donut, boids, vectorscope | ✅ | Black background, glyphs/lines on top |
| bios_post, halftone (low density) | ✅ | Mostly black |
| static, glitch, datamosh, vhs_tracking, crt_collapse | ❌ | Fill screen |
| plasma, mandelbrot, all fractals, voxel_terrain, tunnel | ❌ | Fill screen |
| smoke, pond, caustics | ❌ | Volumetric — background is part of the image |
| bayer, phosphor_crt, synthwave_grid | ❌ | Stylized full coverage |
| pong, pipes_3d, maze_3d | ✅ | Black void around the play area |

**Open questions.**

- **Order vs. PostFX.** If chromakey runs *before* PostFX, bloom / grain /
  CRT overlay tint the key color and break downstream mixer keying. If it
  runs *after*, the operator can't apply bloom to lit cells inside a
  "keyable" scene without bleeding bloom over the key. Default: run
  chromakey **last**, and disable bloom/aberration globally when chromakey
  is on (with a UI warning). The right answer is two output stages: a
  "creative chain" (PostFX) feeds the visible monitor preview, and a
  "broadcast chain" (PostFX + chromakey) feeds the composite-out / HDMI
  going to the mixer. Probably overkill for v1.
- **Per-scene threshold override.** A few scenes might have a dark
  *foreground* pixel that the global threshold accidentally keys (e.g. a
  trail's tail). Per-scene `keyable_luma` overrides the global value.
- **Spill behavior with bright "almost-key" colors.** If a scene happens
  to have a pure green plasma blob, it'd get punched. Solutions: (a) live
  with it — operator picks magenta key; (b) use a chroma-distance match
  in HSV instead of luma threshold. Spec ships (a) and adds (b) only if
  it becomes a problem.

**Test plan.**

- Unit: `chromakey::apply(src_rgba, luma_thresh, key) == src_rgba` when
  every pixel is bright; assert it's `key` everywhere when src is black.
- Integration: render ascii_rain with chromakey enabled + green key,
  assert > 50% of pixels are exactly `0x00FF00`.
- Integration: render plasma with chromakey enabled, assert no pixels
  match the key color (plasma fills the screen with chroma).
- Manual: feed mandleROT composite-out into a hardware mixer with a green
  chroma key; verify ascii_rain over a camera feed reads as floating
  glyphs with no green spill.
