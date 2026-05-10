// Pixel-art anime eyes — matches reference: thick tapered upper lid that
// curves up to the outer corner, thin parallel eyebrow above, thin lower
// lash line below, iris partially covered by lid shadow with horizontal
// color bands (dark → mid → light), pixel-quantized rendering.
//
// Closed-eye blink: upper lid drops to nearly meet lower lash, iris hides.
//
// u_param0  pupil_visible (0..1, 0.0) — 0=no pupil dot in iris, 1=visible
// u_param1  look_speed    (0..1.5, 0.4)
// u_param2  iris_hue      (0..1, 0.58) audio_route=himid amount=0.35
// u_param3  blink_chance  (0..1, 0.45)
// u_param4  eye_size      (0.32..0.55, 0.42)
// u_param5  jitter        (0..0.04, 0.008) audio_route=treble amount=0.5
// u_param6  iris_radius   (0.30..0.55, 0.42) — iris radius as fraction of eye height
// u_param7  pixel_grid    (60..200, 120)

float hash11(float p) {
    return fract(sin(p * 12.9898) * 43758.5453);
}

// Upper lid centerline in local (x_norm, y) coords where x_norm: 0=inner, 1=outer
float upper_lid_y(float x) {
    // Gentle rising curve, slightly faster at outer end.
    return -0.02 + 0.22 * x + 0.06 * x * x;
}
float upper_lid_t(float x) {
    // Tapered thickness: thin at inner, thickest around 0.6, tapers at outer corner.
    return 0.060 * sin(3.14159 * pow(x, 0.55));
}

// Lower lash centerline + thickness
float lower_lash_y(float x) {
    return -0.18 + 0.08 * x;
}
float lower_lash_t(float x) {
    return 0.010 * (1.0 - 0.6 * x);
}

// Eyebrow centerline + thickness (parallel to upper lid, offset up)
float brow_y(float x) {
    return upper_lid_y(x) + 0.14;
}
float brow_t(float x) {
    return 0.014 * sin(3.14159 * pow(x, 0.45));
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
    vec2 lc = vec2(-eye_sz * 1.05, 0.0);
    vec2 rc = vec2( eye_sz * 1.05, 0.0);

    // Look direction
    float lt = u_time * u_param1;
    vec2 look = vec2(sin(lt * 0.7), sin(lt * 0.5 + 1.7)) * 0.5;
    float jt = u_time * 18.0;
    look += vec2(sin(jt), cos(jt * 1.3)) * u_param5 * 6.0;
    look = clamp(look, vec2(-1.0), vec2(1.0));
    vec2 look_offset = look * eye_sz * 0.10;
    look_offset = floor(look_offset / pixel_size) * pixel_size;

    // Blink: 5-frame discrete animation
    float beat_period = 4.0;
    float blink_seed  = floor(beats / beat_period);
    float blink_fire  = step(1.0 - u_param3, hash11(blink_seed));
    float beat_phase  = mod(beats, beat_period);
    float blink_t     = clamp(beat_phase / 0.4, 0.0, 1.0);
    float frame_f     = sin(3.14159 * blink_t) * 4.0 * blink_fire;
    float blink_frame = floor(frame_f + 0.5);
    float blink_norm  = blink_frame / 4.0;

    // Accumulator masks (across both eyes)
    float lid_mask = 0.0;
    float brow_mask = 0.0;
    float lower_lash_mask = 0.0;
    float sclera_mask = 0.0;
    float iris_band1 = 0.0; // top band (darkest in-iris)
    float iris_band2 = 0.0; // middle
    float iris_band3 = 0.0; // bottom (lightest)
    float iris_outline = 0.0;
    float highlight = 0.0;

    for (int i = 0; i < 2; i++) {
        vec2 c = (i == 0) ? lc : rc;
        float outer_sign = (i == 0) ? -1.0 : 1.0;
        vec2 local = uv - c;

        // Mirror x so positive = outer for both eyes
        float xl = local.x * outer_sign;
        float yl = local.y;
        float x_norm = xl / eye_sz;
        float y_norm = yl / eye_sz;

        float in_x = step(0.0, x_norm) * step(x_norm, 1.0);

        // Blink shifts upper lid down toward the lower lash
        float lid_shift = -blink_norm * 0.32;
        float uy = upper_lid_y(x_norm) + lid_shift;
        float ut = upper_lid_t(x_norm);
        float ly = lower_lash_y(x_norm);
        float lt2 = lower_lash_t(x_norm);

        // Upper lid band
        float in_upper = step(abs(y_norm - uy), ut * 0.5) * in_x;
        lid_mask = max(lid_mask, in_upper);

        // Lower lash band
        float in_lower = step(abs(y_norm - ly), lt2 * 0.5) * in_x;
        lower_lash_mask = max(lower_lash_mask, in_lower);

        // Eyebrow band (doesn't blink)
        float by = brow_y(x_norm);
        float bt = brow_t(x_norm);
        float in_brow = step(abs(y_norm - by), bt * 0.5) * in_x;
        brow_mask = max(brow_mask, in_brow);

        // Eye interior between lids (above lower, below upper centerline)
        float between = step(ly + lt2 * 0.5, y_norm) * step(y_norm, uy - ut * 0.5) * in_x;
        sclera_mask = max(sclera_mask, between * step(blink_norm, 0.5));

        // Iris: oval pool seated near the bottom of the interior
        // Position iris center based on x_norm so it sits where the lid arc allows
        float iris_cx = 0.50;
        float interior_top    = uy - ut * 0.5;
        float interior_bottom = ly + lt2 * 0.5;
        float iris_cy = mix(interior_bottom, interior_top, 0.60);
        vec2 iris_center_n = vec2(iris_cx, iris_cy);
        vec2 iris_local_n = vec2(x_norm, y_norm) - iris_center_n;
        // Apply look offset (in normalized eye units)
        iris_local_n -= vec2(look_offset.x * outer_sign / eye_sz,
                              look_offset.y / eye_sz);
        float iris_r = u_param6;
        // Slightly squashed: wider than tall
        float iris_d = length(iris_local_n / vec2(1.0, 0.95)) - iris_r;
        float in_iris_disc = step(iris_d, 0.0) * between * step(blink_norm, 0.5);

        // Iris outline ring (1 pixel band just inside)
        float ring = step(iris_d, 0.0) * step(-0.04, iris_d) * between
                   * step(blink_norm, 0.5);
        iris_outline = max(iris_outline, ring);

        // Color bands within iris: based on y position relative to iris center
        // y_iris_norm in -1..+1 (top of iris = +1, bottom = -1)
        float y_iris = iris_local_n.y / iris_r;
        float band_top    = step(0.30, y_iris) * in_iris_disc;
        float band_mid    = step(-0.20, y_iris) * step(y_iris, 0.30) * in_iris_disc;
        float band_bottom = step(y_iris, -0.20) * in_iris_disc;
        iris_band1 = max(iris_band1, band_top);
        iris_band2 = max(iris_band2, band_mid);
        iris_band3 = max(iris_band3, band_bottom);

        // Highlight: single chunky pixel block in lower portion of iris, slightly inner side
        vec2 hl_pos = iris_local_n - vec2(-0.15, -0.30);
        float hl = step(max(abs(hl_pos.x), abs(hl_pos.y)), 0.10)
                 * in_iris_disc;
        highlight = max(highlight, hl);
    }

    // Palette
    vec3 bg          = vec3(0.97, 0.95, 0.92);
    vec3 lid_color   = vec3(0.03, 0.02, 0.04);
    vec3 sclera_col  = vec3(0.96, 0.94, 0.90);
    float hue_drift  = beats / 32.0;
    // Iris base hue cycles via u_param2 + drift + audio
    vec3 hue_base = 0.5 + 0.5 * cos(
        6.2831 * (u_param2 + hue_drift + vec3(0.0, 0.33, 0.66))
    );
    // Three iris tones: darker → mid → lighter
    vec3 iris_dark = hue_base * 0.35;
    vec3 iris_mid  = hue_base * 0.75;
    vec3 iris_lt   = mix(hue_base, vec3(1.0), 0.45);

    vec3 col = bg;
    col = mix(col, sclera_col,   sclera_mask);
    col = mix(col, iris_dark,    iris_band1);
    col = mix(col, iris_mid,     iris_band2);
    col = mix(col, iris_lt,      iris_band3);
    col = mix(col, iris_dark,    iris_outline);
    col = mix(col, vec3(1.0),    highlight);
    col = mix(col, lid_color,    lid_mask);
    col = mix(col, lid_color,    lower_lash_mask);
    col = mix(col, lid_color,    brow_mask);

    gl_FragColor = vec4(col, 1.0);
}
