// Kawaii-style anime eyes: large pointed almond shape, layered iris with
// radial gradient, multiple sparkle highlights, thick upper eyelash with
// corner spikes, vertical-elongated pupil, pink inner-corner accent.
// Blink animates as an upper-lid sweep-down (not symmetric y-squash) so
// it reads like a real eyelid closing.
//
// u_param0  pupil_size    (0.05..0.18, 0.085) audio_route=bass amount=0.15
// u_param1  look_speed    (0..1.5, 0.45)
// u_param2  iris_hue      (0..1, 0.55) audio_route=himid amount=0.35
// u_param3  blink_chance  (0..1, 0.4)
// u_param4  eye_size      (0.18..0.45, 0.34)
// u_param5  jitter        (0..0.06, 0.010) audio_route=treble amount=0.5
// u_param6  iris_radius   (0.45..0.85, 0.70)
// u_param7  iris_brightness(0.5..1.5, 1.0)

float hash11(float p) {
    return fract(sin(p * 12.9898) * 43758.5453);
}

// Pointed almond eye shape: returns negative inside, positive outside.
// Uses a power norm: (|x/rx|^p + |y/ry|^p) - 1, with p < 2 → pointed corners,
// p = 2 → ellipse. We want pointed kawaii corners.
float almond_sdf(vec2 p, float rx, float ry) {
    vec2 q = vec2(abs(p.x) / rx, abs(p.y) / ry);
    float power = 1.6; // pointed
    return pow(q.x, power) + pow(q.y, power) - 1.0;
}

// Circle SDF
float circ_sdf(vec2 p, vec2 c, float r) {
    return length(p - c) - r;
}

void main() {
    vec2 aspect = vec2(u_resolution.x / u_resolution.y, 1.0);
    vec2 uv = (v_uv * 2.0 - 1.0) * aspect;

    float bpm   = u_bpm > 1.0 ? u_bpm : 120.0;
    float beats = u_time * bpm / 60.0;

    float eye_sz  = u_param4;
    float ery     = eye_sz * 0.62; // eye Y radius (taller than my last attempt)
    float iris_r  = eye_sz * u_param6;
    float pupil_r = u_param0;
    float look_amp = eye_sz * 0.28;

    vec2 lc = vec2(-eye_sz * 1.20, eye_sz * 0.10);
    vec2 rc = vec2( eye_sz * 1.20, eye_sz * 0.10);

    // Look direction: slow Lissajous + treble jitter.
    float lt = u_time * u_param1;
    vec2 look = vec2(sin(lt * 0.7), sin(lt * 0.5 + 1.7)) * 0.55;
    float jt = u_time * 22.0;
    look += vec2(sin(jt), cos(jt * 1.3)) * u_param5 * 5.0;
    look = clamp(look, vec2(-1.0), vec2(1.0));
    vec2 look_offset = look * look_amp;

    // Blink: every 4 beats, with `blink_chance` probability, ~0.35 beat duration.
    float beat_period = 4.0;
    float blink_seed  = floor(beats / beat_period);
    float blink_fire  = step(1.0 - u_param3, hash11(blink_seed));
    float beat_phase  = mod(beats, beat_period);
    float blink_win   = step(beat_phase, 0.35);
    // Smooth sin curve: 0 → 1 → 0 over the window.
    float blink_t = clamp(beat_phase / 0.35, 0.0, 1.0);
    float blink_curve = blink_fire * blink_win * sin(3.14159 * blink_t);

    // Upper-lid Y position. Open: lid at +ery (above eye, no occlusion).
    // Closed: lid sweeps down to -ery (below center, full eye covered).
    float lid_y_off = mix(ery * 1.05, -ery * 0.95, blink_curve);

    // Accumulate masks across the two eyes.
    float eye_mask = 0.0;
    float eye_edge = 0.0;
    float iris_mask = 0.0;
    float iris_rim = 0.0;
    float iris_inner = 0.0;
    float pupil_mask = 0.0;
    float big_highlight = 0.0;
    float small_highlight = 0.0;
    float upper_lash_mask = 0.0;
    float lash_spike_mask = 0.0;
    float lower_lash_mask = 0.0;
    float inner_corner = 0.0;
    float lid_line_mask = 0.0;

    for (int i = 0; i < 2; i++) {
        vec2 c = (i == 0) ? lc : rc;
        vec2 local = uv - c;

        // Almond eye outline (full open shape, no y-squash now)
        float eye_d = almond_sdf(local, eye_sz, ery);
        float full_eye = step(eye_d, 0.0);
        // Lid occlusion: pixels above lid_y_off are covered by the descending lid.
        float below_lid = step(local.y, lid_y_off);
        float e_m = full_eye * below_lid;
        eye_mask = max(eye_mask, e_m);
        eye_edge = max(eye_edge, smoothstep(0.025, 0.0, abs(eye_d)) * full_eye);

        // Iris (offset by look)
        vec2 iris_local = local - look_offset;
        float iris_d = length(iris_local) - iris_r;
        float i_m = step(iris_d, 0.0) * e_m;
        iris_mask = max(iris_mask, i_m);
        // Rim band: outer 20% of iris darker
        float rim_band = step(iris_r * 0.78, length(iris_local))
                       * step(length(iris_local), iris_r) * e_m;
        iris_rim = max(iris_rim, rim_band);
        // Inner brighter band (middle ring)
        float inner_band = step(length(iris_local), iris_r * 0.55)
                         * step(iris_r * 0.30, length(iris_local)) * e_m;
        iris_inner = max(iris_inner, inner_band);

        // Pupil — slightly vertical (taller than wide) for cute look
        vec2 pupil_local = iris_local / vec2(0.85, 1.0);
        float pupil_d = length(pupil_local) - pupil_r;
        pupil_mask = max(pupil_mask, step(pupil_d, 0.0) * e_m);

        // Big highlight: upper-left of iris, fades with blink
        vec2 hl_pos = iris_local - vec2(-iris_r * 0.40, iris_r * 0.40);
        float bh = step(length(hl_pos / vec2(1.3, 1.0)), iris_r * 0.22)
                 * e_m * (1.0 - blink_curve);
        big_highlight = max(big_highlight, bh);

        // Secondary highlight: lower-right small circle
        vec2 hl2_pos = iris_local - vec2(iris_r * 0.30, -iris_r * 0.45);
        float bh2 = step(length(hl2_pos), iris_r * 0.10) * e_m * (1.0 - blink_curve);
        big_highlight = max(big_highlight, bh2);

        // Three tiny sparkle dots scattered in iris
        for (int s = 0; s < 3; s++) {
            float fs = float(s);
            vec2 sparkle_pos = iris_local - vec2(
                (hash11(fs * 7.0 + float(i) * 13.0) - 0.5) * iris_r * 1.4,
                (hash11(fs * 11.0 + float(i) * 17.0) - 0.5) * iris_r * 1.4
            );
            float sd = step(length(sparkle_pos), iris_r * 0.04)
                     * e_m * (1.0 - blink_curve);
            small_highlight = max(small_highlight, sd);
        }

        // Upper eyelash: thick band along the top of the eye almond.
        // The lash follows the descending lid_y_off so it stays "on top" of
        // the visible eye as blink progresses.
        float lash_top    = lid_y_off + eye_sz * 0.04;
        float lash_bottom = lid_y_off - eye_sz * 0.005;
        float horiz_in_eye = step(abs(local.x), eye_sz * 0.95);
        float lash_band = step(lash_bottom, local.y) * step(local.y, lash_top)
                          * horiz_in_eye * full_eye;
        upper_lash_mask = max(upper_lash_mask, lash_band);

        // Outer-corner lash spike: small triangle pointing outward
        float outer_x_sign = (i == 0) ? -1.0 : 1.0;
        vec2 spike_pos = local - vec2(outer_x_sign * eye_sz * 0.85,
                                       lid_y_off + eye_sz * 0.02);
        float spike = step(length(spike_pos), eye_sz * 0.10)
                    * step(spike_pos.x * outer_x_sign, 0.06)
                    * step(-spike_pos.y, 0.05);
        lash_spike_mask = max(lash_spike_mask, spike);

        // Inner-corner pink accent (small wedge near nose)
        float inner_x_sign = (i == 0) ? 1.0 : -1.0;
        vec2 inner_pos = local - vec2(inner_x_sign * eye_sz * 0.75, 0.0);
        float ic = step(length(inner_pos), eye_sz * 0.12)
                 * step(inner_pos.x * inner_x_sign, 0.0) * full_eye;
        inner_corner = max(inner_corner, ic);

        // Lower lash hints: 3 short tick marks below the bottom of the eye
        for (int t = 0; t < 3; t++) {
            float ft = float(t);
            float tick_x = (ft - 1.0) * eye_sz * 0.32;
            vec2 tick_pos = local - vec2(tick_x, -ery * 0.92);
            float tick = step(abs(tick_pos.x), eye_sz * 0.015)
                       * step(abs(tick_pos.y), eye_sz * 0.045);
            lower_lash_mask = max(lower_lash_mask, tick);
        }

        // Mid-blink closed-eye line: when blink_curve high, draw a horizontal
        // arc across the middle of where the eye used to be.
        if (blink_curve > 0.85) {
            float arc_y = local.y - eye_sz * 0.02; // slight downward curve in middle
            float arc_curve = arc_y + (local.x * local.x) / (eye_sz * 2.5);
            float arc_band = step(abs(arc_curve), eye_sz * 0.025)
                           * step(abs(local.x), eye_sz * 0.85);
            lid_line_mask = max(lid_line_mask, arc_band);
        }
    }

    // Colors
    vec3 bg          = vec3(0.05, 0.04, 0.07);
    vec3 sclera      = vec3(0.98, 0.96, 0.95);
    float hue_drift  = beats / 32.0;
    vec3 iris_color  = (0.5 + 0.5 * cos(
        6.2831 * (u_param2 + hue_drift + vec3(0.0, 0.33, 0.66))
    )) * u_param7;
    vec3 iris_rim_color   = iris_color * 0.45;
    vec3 iris_inner_color = mix(iris_color, vec3(1.0), 0.25);
    vec3 pupil_color = vec3(0.05, 0.04, 0.08);
    vec3 lash_color  = vec3(0.04, 0.03, 0.05);
    vec3 pink        = vec3(0.95, 0.55, 0.65);

    vec3 col = bg;
    col = mix(col, sclera,            eye_mask);
    col = mix(col, iris_color,        iris_mask);
    col = mix(col, iris_inner_color,  iris_inner);
    col = mix(col, iris_rim_color,    iris_rim);
    col = mix(col, pupil_color,       pupil_mask);
    col = mix(col, pink,              inner_corner);
    col = mix(col, vec3(1.0),         big_highlight);
    col = mix(col, vec3(1.0),         small_highlight);
    col = mix(col, lash_color,        upper_lash_mask);
    col = mix(col, lash_color,        lash_spike_mask);
    col = mix(col, lash_color,        lower_lash_mask);
    col = mix(col, lash_color,        lid_line_mask);
    col = mix(col, lash_color * 1.6,  eye_edge * (1.0 - iris_mask) * (1.0 - upper_lash_mask));

    gl_FragColor = vec4(col, 1.0);
}
