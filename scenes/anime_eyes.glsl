// Pixel-art anime eyes — per the supplied pixilart reference.
//
// FEATURE INVENTORY (verify each is implemented):
// 1. Eyebrow — bow-curved, thin-thick-thin tapered, extends past outer corner
// 2. Upper lid — asymmetric thick band (fatter above), sharp inner point
// 3. Outer wing — lid continues 0.20 past outer corner as a flick tail
// 4. Inner accent A — short diagonal stroke just inside inner corner (upper)
// 5. Inner accent B — shorter, thinner stroke below accent A
// 6. Lid shadow — solid-black zone under lid filling top of eye interior
// 7. Iris ellipse — lozenge in lower interior
// 8. Iris 4 color bands — dark navy / royal blue / cyan / mint
// 9. Iris highlight — 3x3 pixel cluster, near-white, upper-right of iris
// 10. Lower lash — thin tapered curve, rises to meet wing at outer corner
// 11. Outer-corner V — sharp meeting point of lid wing + lower lash
// 12. Blink — upper lid sweeps down 5 frames; closed shows parallel arcs
//
// u_param0  pupil_size    (0.04..0.14, 0.075) audio_route=bass amount=0.12
// u_param1  look_speed    (0..1.5, 0.4)
// u_param2  iris_hue      (0..1, 0.60) audio_route=himid amount=0.30
// u_param3  blink_chance  (0..1, 0.45)
// u_param4  eye_size      (0.7..1.6, 1.25)
// u_param5  jitter        (0..0.04, 0.008) audio_route=treble amount=0.5
// u_param6  iris_radius   (0.22..0.40, 0.30)
// u_param7  pixel_grid    (80..320, 200)

float hash11(float p) {
    return fract(sin(p * 12.9898) * 43758.5453);
}

// --- Curve definitions (all in normalized eye coords) -----------------------

float upper_lid_y(float x) {
    return -0.05 + 0.50 * x + 0.05 * x * x;
}
// Asymmetric upper-lid thickness: top side bulges more than bottom side.
float upper_lid_t_up(float x) {
    float s = sin(3.14159 * pow(clamp(x, 0.0, 1.20), 0.50));
    // Inner sharp point (x≈0 → t_up≈0); thicker through middle; tapers at wing tip.
    float taper_outer = 1.0 - smoothstep(1.00, 1.20, x); // wing tip thins
    return 0.22 * s * taper_outer;
}
float upper_lid_t_dn(float x) {
    float s = sin(3.14159 * pow(clamp(x, 0.0, 1.20), 0.55));
    float taper_outer = 1.0 - smoothstep(1.00, 1.20, x);
    return 0.13 * s * taper_outer;
}

float lower_lash_y(float x) {
    // Gentle rise + steeper hook near outer where it meets the wing
    float hook = max(0.0, x - 0.85);
    return -0.28 + 0.20 * x + 4.0 * hook * hook;
}
float lower_lash_t(float x) {
    return 0.014 * (1.0 - 0.7 * x);
}

float brow_y(float x) {
    // Bow curve, concave-down (slope decreases toward outer end)
    return 0.50 + 0.40 * pow(clamp(x, 0.0, 1.15), 0.55);
}
float brow_t(float x) {
    float xn = clamp(x, 0.0, 1.15) / 1.15;
    return 0.045 * sin(3.14159 * pow(xn, 0.55));
}

// SDF helper: distance from point p to line segment (a,b)
float seg_dist(vec2 p, vec2 a, vec2 b) {
    vec2 ab = b - a;
    float t = clamp(dot(p - a, ab) / max(dot(ab, ab), 1e-6), 0.0, 1.0);
    return length(p - (a + t * ab));
}

void main() {
    vec2 aspect = vec2(u_resolution.x / u_resolution.y, 1.0);

    float PIX = u_param7;
    vec2 grid = vec2(PIX, PIX / aspect.x);
    vec2 snapped_uv = (floor(v_uv * grid) + 0.5) / grid;
    vec2 uv = (snapped_uv * 2.0 - 1.0) * aspect;
    vec2 pixel_size = aspect * 2.0 / grid;

    float bpm   = u_bpm > 1.0 ? u_bpm : 120.0;
    float beats = u_time * bpm / 60.0;

    float eye_sz = u_param4;
    vec2 lc = vec2(-0.10, 0.0);
    vec2 rc = vec2( 0.10, 0.0);

    float lt = u_time * u_param1;
    vec2 look = vec2(sin(lt * 0.7), sin(lt * 0.5 + 1.7)) * 0.5;
    float jt = u_time * 18.0;
    look += vec2(sin(jt), cos(jt * 1.3)) * u_param5 * 6.0;
    look = clamp(look, vec2(-1.0), vec2(1.0));
    vec2 look_offset = look * eye_sz * 0.07;
    look_offset = floor(look_offset / pixel_size) * pixel_size;

    // Blink: 5 frames
    float beat_period = 4.0;
    float blink_seed  = floor(beats / beat_period);
    float blink_fire  = step(1.0 - u_param3, hash11(blink_seed));
    float beat_phase  = mod(beats, beat_period);
    float blink_t     = clamp(beat_phase / 0.4, 0.0, 1.0);
    float frame_f     = sin(3.14159 * blink_t) * 4.0 * blink_fire;
    float blink_frame = floor(frame_f + 0.5);
    float blink_norm  = blink_frame / 4.0;
    float is_closed = step(blink_norm, 0.99) * step(0.5, blink_norm); // ≥ half-closed

    // Output masks (composited across both eyes)
    float m_lid = 0.0;
    float m_brow = 0.0;
    float m_lower = 0.0;
    float m_inner_a = 0.0;
    float m_inner_b = 0.0;
    float m_shadow = 0.0;
    float m_iris_disc = 0.0;
    float m_iris_a = 0.0;  // dark navy
    float m_iris_b = 0.0;  // royal blue
    float m_iris_c = 0.0;  // medium cyan
    float m_iris_d = 0.0;  // light mint
    float m_highlight = 0.0;
    float m_pupil = 0.0;

    for (int i = 0; i < 2; i++) {
        vec2 c = (i == 0) ? lc : rc;
        float outer_sign = (i == 0) ? -1.0 : 1.0;
        vec2 local = uv - c;

        float xl = local.x * outer_sign;
        float yl = local.y;
        float x_norm = xl / eye_sz;
        float y_norm = yl / eye_sz;

        // X-range gates
        float in_eye_x = step(0.0, x_norm) * step(x_norm, 1.0);
        float in_wing_x = step(1.0, x_norm) * step(x_norm, 1.20);
        float in_lid_x = max(in_eye_x, in_wing_x); // upper lid + wing combined
        float in_brow_x = step(-0.05, x_norm) * step(x_norm, 1.15);
        float in_inner_x = step(-0.16, x_norm) * step(x_norm, 0.02);

        // ── 2. UPPER LID + 3. OUTER WING ──────────────────────────────────
        float lid_shift = -blink_norm * 0.55;
        float uy_center = upper_lid_y(x_norm) + lid_shift;
        float t_up = upper_lid_t_up(x_norm);
        float t_dn = upper_lid_t_dn(x_norm);
        float lid_top    = uy_center + t_up;
        float lid_bottom = uy_center - t_dn;
        float in_upper = step(lid_bottom, y_norm) * step(y_norm, lid_top) * in_lid_x;
        m_lid = max(m_lid, in_upper);

        // ── 10. LOWER LASH ─────────────────────────────────────────────────
        float ly_center = lower_lash_y(x_norm);
        float lash_t = lower_lash_t(x_norm);
        float in_lower = step(abs(y_norm - ly_center), lash_t * 0.5) * in_eye_x;
        m_lower = max(m_lower, in_lower);

        // ── 1. EYEBROW ────────────────────────────────────────────────────
        float by_center = brow_y(x_norm);
        float bt = brow_t(x_norm);
        float in_brow = step(abs(y_norm - by_center), bt * 0.5) * in_brow_x;
        m_brow = max(m_brow, in_brow);

        // ── 4. INNER ACCENT A (upper stroke) ───────────────────────────────
        vec2 acc_a_start = vec2(0.00, -0.05);
        vec2 acc_a_end   = vec2(-0.12, -0.16);
        float acc_a_d = seg_dist(vec2(x_norm, y_norm), acc_a_start, acc_a_end);
        float acc_a_taper = 1.0 - smoothstep(0.0, length(acc_a_end - acc_a_start),
                                              seg_dist(vec2(x_norm, y_norm),
                                                       acc_a_start, acc_a_start));
        float in_acc_a = step(acc_a_d, 0.012) * in_inner_x * (1.0 - is_closed);
        m_inner_a = max(m_inner_a, in_acc_a);

        // ── 5. INNER ACCENT B (lower stroke) ───────────────────────────────
        vec2 acc_b_start = vec2(-0.02, -0.20);
        vec2 acc_b_end   = vec2(-0.12, -0.26);
        float acc_b_d = seg_dist(vec2(x_norm, y_norm), acc_b_start, acc_b_end);
        float in_acc_b = step(acc_b_d, 0.009) * in_inner_x;
        m_inner_b = max(m_inner_b, in_acc_b);

        // ── 6. LID SHADOW ─────────────────────────────────────────────────
        // Bounded above by lid_bottom, below by a slight curve mirroring iris top
        // Shadow region: between bottom of lid and a margin above iris center
        float shadow_floor_y = -0.05 + 0.08 * x_norm; // mild slope
        float in_shadow = step(shadow_floor_y, y_norm) * step(y_norm, lid_bottom)
                          * in_eye_x * (1.0 - is_closed);
        m_shadow = max(m_shadow, in_shadow);

        // ── 7. IRIS DISC ──────────────────────────────────────────────────
        vec2 iris_center = vec2(0.50, -0.05);
        vec2 iris_local = vec2(x_norm, y_norm) - iris_center;
        iris_local -= vec2(look_offset.x * outer_sign / eye_sz,
                           look_offset.y / eye_sz);
        float iris_r = u_param6;
        // Squashed ellipse: wider than tall
        float iris_d = length(iris_local / vec2(1.0, 0.65)) - iris_r;
        float in_iris = step(iris_d, 0.0) * (1.0 - is_closed)
                        * step(y_norm, lid_bottom); // never overlap lid
        m_iris_disc = max(m_iris_disc, in_iris);

        // ── 8. IRIS COLOR BANDS ───────────────────────────────────────────
        // y_iris in [-1, +1] (rough), with 0 at iris center
        float y_iris = iris_local.y / (iris_r * 0.65);
        m_iris_a = max(m_iris_a, step(0.50, y_iris) * in_iris);
        m_iris_b = max(m_iris_b, step(0.10, y_iris) * step(y_iris, 0.50) * in_iris);
        m_iris_c = max(m_iris_c, step(-0.30, y_iris) * step(y_iris, 0.10) * in_iris);
        m_iris_d = max(m_iris_d, step(y_iris, -0.30) * in_iris);

        // ── 9. IRIS HIGHLIGHT ─────────────────────────────────────────────
        vec2 hl_pos = iris_local - vec2(iris_r * 0.50, iris_r * 0.10);
        float hl = step(max(abs(hl_pos.x), abs(hl_pos.y)), iris_r * 0.16)
                 * in_iris;
        m_highlight = max(m_highlight, hl);

        // pupil — subtle, hidden under shadow but defined for completeness
        float pupil_d = length(iris_local / vec2(0.85, 1.0)) - u_param0;
        m_pupil = max(m_pupil, step(pupil_d, 0.0) * in_iris);
    }

    // ── COLOR PALETTE ──────────────────────────────────────────────────────
    vec3 bg          = vec3(0.97, 0.95, 0.92);
    vec3 lid_color   = vec3(0.03, 0.02, 0.05);
    float hue_drift  = beats / 32.0;
    // Base hue cycles
    float hue = u_param2 + hue_drift;
    // 4 iris tones: anchor to "blue family" and shift with hue param
    vec3 navy = 0.5 + 0.5 * cos(6.2831 * (hue + vec3(0.0, 0.33, 0.66))); // base color
    vec3 c_navy  = navy * 0.20;
    vec3 c_royal = navy * 0.55;
    vec3 c_cyan  = mix(navy, vec3(0.40, 0.70, 0.95), 0.5);
    vec3 c_mint  = mix(navy, vec3(0.55, 0.92, 0.95), 0.65);
    vec3 c_hl    = vec3(0.95, 0.98, 0.92); // near-white

    // ── COMPOSITE ─────────────────────────────────────────────────────────
    // Render order (later overrides earlier):
    //   bg → iris bands → highlight → pupil → shadow → lower lash →
    //   eyebrow → inner accents → upper lid (top)
    vec3 col = bg;
    col = mix(col, c_navy,    m_iris_a);
    col = mix(col, c_royal,   m_iris_b);
    col = mix(col, c_cyan,    m_iris_c);
    col = mix(col, c_mint,    m_iris_d);
    col = mix(col, c_hl,      m_highlight);
    col = mix(col, lid_color, m_shadow);
    col = mix(col, lid_color, m_lower);
    col = mix(col, lid_color, m_brow);
    col = mix(col, lid_color, m_inner_a);
    col = mix(col, lid_color, m_inner_b);
    col = mix(col, lid_color, m_lid);

    gl_FragColor = vec4(col, 1.0);
}
