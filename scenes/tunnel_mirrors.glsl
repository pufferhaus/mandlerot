// u_param0  segments      (3..16, 6)   — kaleidoscope mirror count (int-ish)
// u_param1  speed         (0..2, 0.5)  — tunnel fly-through speed [bass 0.4]
// u_param2  twist         (-2..2, 0.3) — angular twist over depth
// u_param3  hue           (0..1, 0.65) — base palette hue
// u_param4  stripe_density (1..30, 8)  — depth stripe count
//
// Kaleidoscope-on-tunnel: polar coords folded into N mirror segments,
// then depth-striped with a cosine palette. Fly-through is BPM-locked.
void main() {
    float bpm  = u_bpm > 1.0 ? u_bpm : 120.0;
    float beat_time = u_time * bpm / 60.0;

    float segs   = floor(u_param0);
    float speed  = u_param1;
    float twist  = u_param2;
    float hue    = u_param3;
    float stripe = u_param4;

    float TAU = 6.28318;
    float aspect = u_resolution.x / u_resolution.y;
    vec2 c = (v_uv - 0.5) * vec2(aspect, 1.0);

    float radius = length(c);
    float angle  = atan(c.y, c.x); // -pi..pi

    // Kaleidoscope fold: mirror into one segment of width TAU/segs
    float seg_angle = TAU / segs;
    float folded = mod(angle, seg_angle);
    folded = abs(folded - seg_angle * 0.5); // 0 at mirror line, max at edge

    // Twist: rotate with depth
    float depth = 1.0 / max(radius, 0.05);
    float tv = depth * 0.3 - beat_time * speed * (1.0 / 16.0);
    folded += twist * depth * 0.03;

    // Procedural pattern in folded (angle) × depth space
    float pu = folded * segs; // scale angle span
    float pv = tv * stripe;

    float pat = sin(pu * 3.14159) * cos(pv * 3.14159);
    pat = 0.5 + 0.5 * pat;

    // Audio drift, hue, and slow BPM-locked palette drift.
    float audio_drift = u_audio.x * 0.12 + u_beat * 0.04;
    float bpm_drift = beat_time / 32.0;
    float col_t = pat * 0.4 + hue + audio_drift + bpm_drift + folded * 0.1;
    vec3 col = 0.5 + 0.5 * cos(6.2831 * (col_t + vec3(0.0, 0.33, 0.66)));

    // Vignette toward center (the vanishing point)
    float vig = clamp(radius * 2.0, 0.0, 1.0);
    col *= vig;

    gl_FragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
