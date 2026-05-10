// u_param0  scale          (4..40, 16)  — hex cell density (integer-ish)
// u_param1  hue            (0..1, 0.4)  — base hue [lomid 0.3]
// u_param2  cell_brightness(0.3..1.0, 0.7) — cell fill brightness [bass 0.4]
// u_param3  edge_glow      (0..0.5, 0.15) — edge highlight width
// u_param4  anim_speed     (0..2, 0.5)  — per-cell color cycle speed
//
// Hexagonal tile pattern using staggered-row (offset) axial coordinates.
// Each cell is colored by a hash on cell index + time*anim_speed.
float hxHash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }

void main() {
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;

    float scale      = u_param0;
    float hue        = u_param1;
    float brightness = u_param2;
    float edge_glow  = u_param3;
    float anim_speed = u_param4;

    vec2 uv = v_uv * scale;

    // Staggered-row hex grid: offset odd rows by 0.5
    float row = floor(uv.y);
    float col_offset = mod(row, 2.0) * 0.5;
    vec2 q = vec2(uv.x + col_offset, uv.y);
    vec2 cell = floor(q);
    vec2 fr = fract(q) - 0.5;

    float dist_center = length(fr);

    // Per-cell color: hash of cell coords + animated time bucket
    // BPM-locked drift so palette evolves continuously with no audio.
    float time_bucket = floor(u_time * (0.5 + anim_speed) * 1.5);
    float h = hxHash(cell + time_bucket);
    float cell_hue = hue + h * 0.4 + u_time * bpm / (60.0 * 32.0);
    vec3 col = 0.5 + 0.5 * cos(6.2831 * (cell_hue + vec3(0.0, 0.33, 0.66)));

    // Edge glow: bright ring near cell boundary
    float edge = 1.0 - smoothstep(0.4 - edge_glow, 0.5, dist_center);
    float fill = 1.0 - smoothstep(0.3, 0.45, dist_center);

    float lit = fill * brightness + edge * edge_glow * 2.0;
    lit = clamp(lit, 0.0, 1.5);

    // Beat flash
    lit *= 1.0 + u_beat * 0.3;

    col = clamp(col * lit, 0.0, 1.0);
    gl_FragColor = vec4(col, 1.0);
}
