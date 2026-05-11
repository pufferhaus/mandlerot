// CRT signal collapse: scanlines, chromatic aberration, vertical-hold roll,
// and an occasional static-snow band. The "base" is an audio-driven RGB
// gradient so the effects always have something to corrupt.
//
// u_param0 scan_depth   0..1   scanline contrast (0.6)
// u_param1 aberration   0..1   chromatic split width  [treble +]
// u_param2 roll_rate    0..1   chance of vertical-hold roll per beat (0.5)
// u_param3 roll_speed   0..4   how fast the picture rolls when triggered (1.5)
// u_param4 snow_amount  0..1   density of static-snow bands  [treble +]
// u_param5 hue          0..1   base palette hue
// u_param6 brightness   0..2   global gain
// u_param7 tint_warp    0..1   how much audio warps the hue

float hash21(vec2 p){ return fract(sin(dot(p, vec2(12.9898,78.233))) * 43758.5453); }

vec3 base_color(vec2 uv) {
    // Smooth audio-driven gradient: bass → x, treble → y.
    float h = u_param5 + u_audio.x * 0.4 - u_audio.w * 0.2 + u_param7 * sin(u_time * 0.3);
    float v = mix(uv.x, uv.y, 0.4 + 0.3 * sin(u_time * 0.4));
    vec3 c = 0.5 + 0.5 * cos(6.2831 * (h + v + vec3(0.0, 0.33, 0.66)));
    return c;
}

void main() {
    vec2 uv = v_uv;

    // ---- vertical-hold roll: occasionally shifts uv.y for a beat
    // Triggered probabilistically on beat. Wraps as it rolls.
    float roll_phase = floor(u_time * 1.7);
    float roll_active = step(1.0 - u_param2, hash21(vec2(roll_phase, 7.0)));
    float roll = roll_active * fract(u_time * u_param3);
    uv.y = fract(uv.y + roll);

    // ---- chromatic aberration: sample base at uv ± k for R/B
    float ca = u_param1 * (0.005 + 0.02 * u_audio.w);
    vec3 col;
    col.r = base_color(uv + vec2(ca, 0.0)).r;
    col.g = base_color(uv).g;
    col.b = base_color(uv - vec2(ca, 0.0)).b;

    // ---- scanlines: horizontal sin modulation, sharp
    float sl = 0.5 + 0.5 * sin(uv.y * u_resolution.y * 3.1415);
    col *= mix(1.0, sl, u_param0);

    // ---- static snow bands: random rows of noise (treble drives count)
    float row = floor(uv.y * 60.0);
    float band_seed = hash21(vec2(row, floor(u_time * 6.0)));
    float band_active = step(1.0 - u_param4 * 0.4, band_seed);
    float snow = hash21(uv * u_resolution.xy + u_time);
    col = mix(col, vec3(snow), band_active * 0.7);

    // ---- subtle vignette so the screen feels bowed
    float vig = 1.0 - 0.6 * length((v_uv - 0.5) * vec2(u_resolution.x / u_resolution.y, 1.0));
    col *= clamp(vig, 0.3, 1.0);

    col *= u_param6;
    gl_FragColor = vec4(col, 1.0);
}
