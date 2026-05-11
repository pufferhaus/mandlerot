// Audio vectorscope. mandleROT only captures mono audio, so a true L/R
// Lissajous isn't available. Instead we plot two band pairs (bass vs
// treble, lomid vs himid) over the last N frames using u_audio_history
// so the trace has visible persistence.
//
// u_param0 trail_len   8..256   number of history samples plotted (128)
// u_param1 trace_width 0.002..0.05 line thickness (0.012)
// u_param2 glow        0..1     additive bloom around the trace (0.5)
// u_param3 scale       0.3..1.5 amplitude scale (0.9)
// u_param4 hue_a       0..1     first trace color (cyan)
// u_param5 hue_b       0..1     second trace color (magenta)
// u_param6 graticule   0..1     graticule grid visibility (0.4)
// u_param7 brightness  0..2     global gain

void main() {
    vec2 uv = v_uv - 0.5;
    uv.x *= u_resolution.x / u_resolution.y;

    vec3 tint_a = 0.5 + 0.5 * cos(6.2831 * (u_param4 + vec3(0.0, 0.33, 0.66)));
    vec3 tint_b = 0.5 + 0.5 * cos(6.2831 * (u_param5 + vec3(0.0, 0.33, 0.66)));
    vec3 col = vec3(0.0);

    // graticule: cross + circle at radius 0.4
    if (u_param6 > 0.0) {
        float cross = smoothstep(0.003, 0.0, abs(uv.x))
                    + smoothstep(0.003, 0.0, abs(uv.y));
        float circle = smoothstep(0.003, 0.0, abs(length(uv) - 0.4));
        col += vec3(0.15, 0.20, 0.18) * (cross + circle) * u_param6;
    }

    float n = max(u_param0, 8.0);
    float w = u_param1;
    float scale = u_param3;
    for (int i = 0; i < 256; i++) {
        if (float(i) >= n) break;
        // v=1 is newest, v=0 oldest; sample evenly across our window
        float v = 1.0 - (float(i) / (n - 1.0)) * 0.5; // last half of history
        vec4 row = texture2D(u_audio_history, vec2(0.5, v));
        // Trace A: (bass, treble) → (x, y), centered on origin
        vec2 a = vec2(row.r - 0.5, row.a - 0.5) * scale;
        vec2 b = vec2(row.g - 0.5, row.b - 0.5) * scale;
        float fade = 1.0 - float(i) / n;

        float da = length(uv - a);
        float db = length(uv - b);
        col += tint_a * smoothstep(w + 0.005, 0.0, da) * fade * 0.6;
        col += tint_b * smoothstep(w + 0.005, 0.0, db) * fade * 0.6;
        // glow
        col += tint_a * exp(-da * 40.0) * fade * u_param2 * 0.5;
        col += tint_b * exp(-db * 40.0) * fade * u_param2 * 0.5;
    }

    col *= u_param7;
    gl_FragColor = vec4(col, 1.0);
}
