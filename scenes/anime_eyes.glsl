// Pixel-art anime eyes (chibi sprite style).
// Quantizes UV to a low-resolution grid so every shape edge falls on a pixel
// boundary. Hard step() transitions only, no smoothstep. Limited 6-color
// palette. Blink animates in discrete frames (open → mid → squint → closed)
// instead of a smooth curve, matching pixel-art sprite animation timing.
//
// u_param0  pupil_size    (0.05..0.20, 0.10) audio_route=bass amount=0.15
// u_param1  look_speed    (0..1.5, 0.4)
// u_param2  iris_hue      (0..1, 0.55) audio_route=himid amount=0.35
// u_param3  blink_chance  (0..1, 0.4)
// u_param4  eye_size      (0.20..0.50, 0.36)
// u_param5  jitter        (0..0.06, 0.012) audio_route=treble amount=0.5
// u_param6  iris_radius   (0.55..0.85, 0.72)
// u_param7  pixel_grid    (40..160, 96) — logical resolution; lower = chunkier

float hash11(float p) {
    return fract(sin(p * 12.9898) * 43758.5453);
}

float almond_sdf(vec2 p, float rx, float ry) {
    vec2 q = vec2(abs(p.x) / rx, abs(p.y) / ry);
    return pow(q.x, 1.6) + pow(q.y, 1.6) - 1.0;
}

void main() {
    vec2 aspect = vec2(u_resolution.x / u_resolution.y, 1.0);

    // Snap UV to pixel grid. PIX is the logical horizontal resolution; the
    // vertical follows aspect so pixels stay square in worldspace.
    float PIX = u_param7;
    vec2 grid = vec2(PIX, PIX / aspect.x);
    vec2 snapped_uv = (floor(v_uv * grid) + 0.5) / grid;
    vec2 uv = (snapped_uv * 2.0 - 1.0) * aspect;

    float bpm   = u_bpm > 1.0 ? u_bpm : 120.0;
    float beats = u_time * bpm / 60.0;

    float eye_sz  = u_param4;
    float ery     = eye_sz * 0.60;
    float iris_r  = eye_sz * u_param6;
    float pupil_r = u_param0;
    float look_amp = eye_sz * 0.25;

    vec2 lc = vec2(-eye_sz * 1.20, eye_sz * 0.05);
    vec2 rc = vec2( eye_sz * 1.20, eye_sz * 0.05);

    // Snap the look offset to whole logical pixels too so the pupil moves in
    // discrete steps, classic sprite animation.
    float lt = u_time * u_param1;
    vec2 look = vec2(sin(lt * 0.7), sin(lt * 0.5 + 1.7)) * 0.55;
    float jt = u_time * 18.0;
    look += vec2(sin(jt), cos(jt * 1.3)) * u_param5 * 5.0;
    look = clamp(look, vec2(-1.0), vec2(1.0));
    vec2 look_offset = look * look_amp;
    vec2 pixel_size = aspect * 2.0 / grid;
    look_offset = floor(look_offset / pixel_size) * pixel_size;

    // Blink in discrete frames: 0=open, 1=quarter, 2=half, 3=three-quarter, 4=closed.
    // Each frame holds for ~1 panel-frame at default cadence.
    float beat_period = 4.0;
    float blink_seed  = floor(beats / beat_period);
    float blink_fire  = step(1.0 - u_param3, hash11(blink_seed));
    float beat_phase  = mod(beats, beat_period);
    // 0.4 beat blink duration; map to discrete frames 0..4..0
    float blink_t = clamp(beat_phase / 0.4, 0.0, 1.0);
    float frame_f = sin(3.14159 * blink_t) * 4.0 * blink_fire;
    float blink_frame = floor(frame_f + 0.5); // 0..4 integer
    float blink_norm = blink_frame / 4.0;     // 0..1 snapped

    // Upper-lid Y position snapped to frames
    float lid_y_off = mix(ery * 1.10, -ery * 1.05, blink_norm);
    // Snap lid Y to grid too
    lid_y_off = floor(lid_y_off / pixel_size.y) * pixel_size.y;

    float eye_mask = 0.0;
    float eye_edge = 0.0;
    float iris_mask = 0.0;
    float iris_rim = 0.0;
    float pupil_mask = 0.0;
    float highlight_mask = 0.0;
    float upper_lash_mask = 0.0;
    float outer_corner_lash = 0.0;
    float closed_line = 0.0;

    for (int i = 0; i < 2; i++) {
        vec2 c = (i == 0) ? lc : rc;
        vec2 local = uv - c;

        float eye_d = almond_sdf(local, eye_sz, ery);
        float full_eye = step(eye_d, 0.0);
        float below_lid = step(local.y, lid_y_off);
        float e_m = full_eye * below_lid;
        eye_mask = max(eye_mask, e_m);

        // 1-pixel-thick outline: just inside the almond boundary
        float edge_band = step(eye_d, 0.0) * step(-0.06, eye_d);
        eye_edge = max(eye_edge, edge_band);

        // Iris
        vec2 iris_local = local - look_offset;
        float iris_d = length(iris_local) - iris_r;
        iris_mask = max(iris_mask, step(iris_d, 0.0) * e_m);

        // Darker rim band (single-pixel ring just inside iris edge)
        float rim_inner = iris_r * 0.82;
        float rim_d = length(iris_local);
        iris_rim = max(iris_rim,
            step(rim_inner, rim_d) * step(rim_d, iris_r) * e_m);

        // Pupil
        pupil_mask = max(pupil_mask,
            step(length(iris_local), pupil_r) * e_m);

        // Single chunky highlight pixel block in upper-left of iris
        vec2 hl_pos = iris_local - vec2(-iris_r * 0.38, iris_r * 0.38);
        float h = step(max(abs(hl_pos.x), abs(hl_pos.y)), iris_r * 0.16)
                * e_m * (1.0 - step(0.5, blink_norm));
        highlight_mask = max(highlight_mask, h);

        // Second smaller highlight square
        vec2 hl2_pos = iris_local - vec2(-iris_r * 0.10, iris_r * 0.55);
        float h2 = step(max(abs(hl2_pos.x), abs(hl2_pos.y)), iris_r * 0.06)
                 * e_m * (1.0 - step(0.5, blink_norm));
        highlight_mask = max(highlight_mask, h2);

        // Upper eyelash: thick band riding on top of the descending lid
        float lash_top    = lid_y_off + eye_sz * 0.08;
        float lash_bottom = lid_y_off;
        float horiz = step(abs(local.x), eye_sz * 0.95);
        upper_lash_mask = max(upper_lash_mask,
            step(lash_bottom, local.y) * step(local.y, lash_top) * horiz * full_eye);

        // Outer-corner lash: 2x2-ish pixel block sticking out from the corner
        float outer_x_sign = (i == 0) ? -1.0 : 1.0;
        vec2 corner_pos = local - vec2(outer_x_sign * eye_sz * 0.95,
                                         lid_y_off + eye_sz * 0.02);
        float corner = step(max(abs(corner_pos.x), abs(corner_pos.y)), eye_sz * 0.08);
        outer_corner_lash = max(outer_corner_lash, corner);

        // Fully-closed mid-eye line at blink_frame == 4
        float is_closed = step(3.5, blink_frame);
        float closed_band = step(abs(local.y - eye_sz * 0.0), eye_sz * 0.04)
                          * step(abs(local.x), eye_sz * 0.85) * is_closed;
        closed_line = max(closed_line, closed_band);
    }

    // 6-color palette: hard transitions only.
    vec3 bg          = vec3(0.05, 0.04, 0.09);
    vec3 sclera      = vec3(0.97, 0.96, 0.90);
    float hue_drift  = beats / 32.0;
    vec3 iris_color  = 0.5 + 0.5 * cos(
        6.2831 * (u_param2 + hue_drift + vec3(0.0, 0.33, 0.66))
    );
    vec3 iris_rim_color = iris_color * 0.55;
    vec3 pupil_color = vec3(0.03, 0.02, 0.08);
    vec3 lash_color  = vec3(0.04, 0.03, 0.06);

    vec3 col = bg;
    col = mix(col, sclera,            eye_mask);
    col = mix(col, iris_color,        iris_mask);
    col = mix(col, iris_rim_color,    iris_rim);
    col = mix(col, pupil_color,       pupil_mask);
    col = mix(col, vec3(1.0),         highlight_mask);
    col = mix(col, lash_color,        upper_lash_mask);
    col = mix(col, lash_color,        outer_corner_lash);
    col = mix(col, lash_color,        eye_edge * (1.0 - iris_mask) * (1.0 - upper_lash_mask));
    col = mix(col, lash_color,        closed_line);

    gl_FragColor = vec4(col, 1.0);
}
