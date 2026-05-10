// u_param0  grid_n       (4..32, 12)  — number of cells per axis (integer-ish)
// u_param1  dot_size     (0.1..0.9, 0.6) — dot diameter as fraction of cell
// u_param2  hue          (0..1, 0.7)  — base hue
// u_param3  pulse_amount (0..1, 0.6)  — beat brightness boost
// u_param4  hue_drift    (0..2, 0.3)  — color cycle speed (BPM-locked)
//
// NxN grid of dots. Each cell pulses brightness on beat; color is hue-shifted
// per cell using a hash on (col+row). Audio band selection also varies per cell.
float pgHash(float n) { return fract(sin(n * 127.1) * 43758.5453); }

void main() {
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;
    float beat_phase = u_time * bpm / 60.0;

    float grid_n     = u_param0;
    float dot_size   = u_param1;
    float hue        = u_param2;
    float pulse_amt  = u_param3;
    float hue_drift  = u_param4;

    vec2 uv = v_uv;

    // Discretize into cells
    vec2 cell_uv  = uv * grid_n;
    vec2 cell_id  = floor(cell_uv);
    vec2 cell_frac = fract(cell_uv) - 0.5; // [-0.5, 0.5] inside cell

    float h = pgHash(cell_id.x + cell_id.y * 31.7);
    float h2 = pgHash(cell_id.x * 17.3 + cell_id.y * 5.1 + 7.0);

    // Pick audio band by hash
    float band;
    float bsel = h2 * 4.0;
    if (bsel < 1.0) {
        band = u_audio.x;
    } else if (bsel < 2.0) {
        band = u_audio.y;
    } else if (bsel < 3.0) {
        band = u_audio.z;
    } else {
        band = u_audio.w;
    }

    // Dot: circular with dot_size controlling radius
    float dist = length(cell_frac);
    float radius = dot_size * 0.5;
    float dot = 1.0 - smoothstep(radius - 0.02, radius, dist);

    // Beat pulse: u_beat is 0..1 envelope, boost brightness
    float brightness = 0.4 + band * 0.4 + u_beat * pulse_amt;

    // Per-cell color from hash + time drift
    float cell_hue = hue + h * 0.3 + beat_phase * hue_drift * 0.05;
    vec3 col = 0.5 + 0.5 * cos(6.2831 * (cell_hue + vec3(0.0, 0.33, 0.66)));

    gl_FragColor = vec4(col * dot * brightness, 1.0);
}
