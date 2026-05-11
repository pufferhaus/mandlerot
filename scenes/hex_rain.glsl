// Hex/binary rain. Reuses the ascii_rain skeleton but glyphs are 0-9, A-F
// (or 0/1 in binary mode), denser columns, slower fall — reads as sysadmin
// console rather than anime.
//
// u_param0 speed         0.1..3.0  scroll speed (0.5)
// u_param1 glyph_rate    1..12     glyph mutation Hz (4)
// u_param2 density       0..1      fraction of active columns (0.7)
// u_param3 trail_decay   0.04..0.40 fade rate (0.18)
// u_param4 palette_shift 0..1      0 = amber BIOS, 1 = green terminal
// u_param5 head_bright   0..1      head cell brightness mix (0.7)
// u_param6 binary_mix    0..1      0 = hex chars, 1 = binary digits
// u_param7 noise_amount  0..1      sparse stray glyphs in dead columns

float h11(float x){ return fract(sin(x * 12.9898) * 43758.5453); }
float h21(vec2 p){ return fract(sin(dot(p, vec2(12.9898,78.233))) * 43758.5453); }

// 5x7 bitmap for digits 0-9 and A-F. Each glyph id 0..15 picks a pattern;
// pattern bit is set when the hash thresholds for the (id, sub) pair.
// Stylized — not perfectly readable as digits, but conveys "data console."
float glyph_pixel(float id, vec2 sub) {
    float h = h21(vec2(id * 13.0 + sub.x * 5.0, sub.y));
    return step(0.5, h);
}

void main() {
    float cell = 9.0;
    vec2 px = v_uv * u_resolution;
    vec2 grid = floor(px / cell);
    float col = grid.x;
    float rows = u_resolution.y / cell;

    float col_active = step(1.0 - u_param2, h11(col * 0.7 + 99.0));
    float col_speed = (0.5 + 1.5 * h11(col + 17.0)) * u_param0;
    float col_offset = h11(col + 31.0);
    float trail_len = (0.25 + 0.55 * h11(col + 53.0)) * rows;

    // head: cells/sec independent of resolution
    float head_y = mod(u_time * col_speed * 6.0 + col_offset * rows, rows);
    float dist_from_head = head_y - grid.y;
    float dist_wrapped = mod(dist_from_head, rows);

    float fade_rate = max(u_param3, 0.01);
    float in_trail = step(0.0, dist_from_head + 0.0001) * step(dist_wrapped, trail_len);
    float bright = exp(-dist_wrapped * fade_rate) * in_trail * col_active;

    // glyph: 5x7 sub-pixel bitmap inside the cell
    vec2 cell_uv = fract(px / cell);
    cell_uv = (cell_uv - 0.5) * 1.10 + 0.5;
    vec2 sub = floor(cell_uv * vec2(5.0, 7.0));
    float in_bounds = step(0.0, sub.x) * step(0.0, sub.y)
                    * step(sub.x, 4.0) * step(sub.y, 6.0);

    float gr = max(u_param1, 0.5);
    float t_quant_head = floor(u_time * gr);
    float t_quant_trail = floor(u_time * gr * 0.15);
    float at_head = step(dist_wrapped, 0.5);
    float t_quant = mix(t_quant_trail, t_quant_head, at_head);
    float glyph_id_raw = h21(grid + vec2(t_quant, 0.0));
    // hex (0..15) or binary (0..1) selection
    float glyph_id = mix(floor(glyph_id_raw * 16.0), floor(glyph_id_raw * 2.0), u_param6);
    float pixel_on = glyph_pixel(glyph_id, sub) * in_bounds;

    // palette: amber → green blend
    vec3 amber = vec3(1.00, 0.70, 0.10);
    vec3 green = vec3(0.10, 1.00, 0.30);
    vec3 tint = mix(amber, green, u_param4);

    vec3 trail_color = tint * bright;
    vec3 head_color = mix(tint, vec3(1.0, 1.0, 0.95), u_param5);
    vec3 color = mix(trail_color, head_color, at_head * col_active);
    color *= pixel_on;

    // sparse stray glyphs in dead columns: hash-gated stand-alone characters
    float stray_seed = h21(grid + vec2(floor(u_time * 2.0), 0.0));
    float stray = step(1.0 - u_param7 * 0.02, stray_seed) * (1.0 - col_active);
    color += tint * 0.35 * pixel_on * stray;

    // tiny scanline so it reads as a CRT console
    color *= 0.85 + 0.15 * sin(v_uv.y * u_resolution.y * 3.1415);

    gl_FragColor = vec4(color, 1.0);
}
