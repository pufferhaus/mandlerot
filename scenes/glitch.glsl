// u_param0  rows            (8..64) — number of horizontal scan bands
// u_param1  shift_amount    (0..0.3) — max horizontal shift per band [beat]
// u_param2  chroma_split    (0..0.05) — RGB channel separation [treble]
// u_param3  hue_base        (0..1)  — base hue tint
// u_param4  scanline_strength (0..1) — scanline darkening
//
// Glitch/sync-roll: per-band horizontal shift driven by beat.
// Hash function produces pseudo-random per-band offsets.
float hash(float n) {
    return fract(sin(n) * 43758.5453);
}
void main() {
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;
    float rows = floor(u_param0);
    float shift = u_param1;
    float chroma = u_param2;
    float hue = u_param3;
    float scanlines = u_param4;

    float band = floor(v_uv.y * rows);
    float beat_seed = floor(u_time * bpm / 60.0); // changes each beat
    float rnd = hash(band * 17.3 + beat_seed * 3.7);
    float row_shift = (rnd - 0.5) * 2.0 * shift * (0.3 + 0.7 * u_beat);

    // Sample each channel with slight offset for chroma split
    vec2 uvR = vec2(fract(v_uv.x + row_shift + chroma), v_uv.y);
    vec2 uvG = vec2(fract(v_uv.x + row_shift),          v_uv.y);
    vec2 uvB = vec2(fract(v_uv.x + row_shift - chroma), v_uv.y);

    // Base pattern: trig stripes that look like video noise
    float t = u_time * 0.5;
    float r = 0.5 + 0.5 * sin(uvR.y * 40.0 + t * 3.1 + hash(band) * 6.28);
    float g = 0.5 + 0.5 * sin(uvG.y * 40.0 + t * 2.7 + hash(band + 1.0) * 6.28);
    float b = 0.5 + 0.5 * sin(uvB.y * 40.0 + t * 3.7 + hash(band + 2.0) * 6.28);

    vec3 col = vec3(r, g, b);
    // Tint with hue
    vec3 tint = 0.5 + 0.5 * cos(6.2831 * (hue + vec3(0.0, 0.33, 0.66)));
    col = mix(col, col * tint * 2.0, 0.4);

    // Scanlines
    float scan = 1.0 - scanlines * 0.5 * (0.5 + 0.5 * sin(v_uv.y * u_resolution.y * 3.14159));
    col *= scan;

    gl_FragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
