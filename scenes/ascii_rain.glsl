// Matrix-style digital rain. Each cell contains a faux glyph rendered as
// a 5x7 sub-pixel bitmap whose on/off pattern is hashed from a glyph id.
// Glyph id mutates slowly per cell (more often near the falling head).
// Trail: head is bright white, trail fades green exponentially.
// Columns are sparse: ~25% of columns are empty at any time (hash gate).
// Trail length and speed vary per column.
//
// u_param0  speed        (0.3..3, 1.0)   [bass +]
// u_param1  glyph_rate   (2..18, 8)      — glyph cycle Hz [lomid +]
// u_param2  tint_shift   (0..1, 0.05)    — small hue offset from pure green
// u_param3  trail_decay  (0.04..0.40, 0.18) [treble -]

float hash11(float x){ return fract(sin(x * 12.9898) * 43758.5453); }
float hash21(vec2 p){ return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453); }

void main(){
    float cell = 10.0;
    vec2 px = v_uv * u_resolution;
    vec2 grid = floor(px / cell);
    float col = grid.x;

    float rows = u_resolution.y / cell;

    // ---- per-column character ----
    float col_active   = step(0.25, hash11(col * 0.7 + 99.0));      // 75% of columns alive
    float col_speed    = (0.5 + 1.5 * hash11(col + 17.0)) * u_param0;
    float col_offset   = hash11(col + 31.0);
    float trail_len    = (0.25 + 0.55 * hash11(col + 53.0)) * rows;

    // head position (modular). col_speed is in "cells per second" so the
    // pace is independent of window height — slower screens are not also
    // slower rain. Default speeds give ~3..12 cells/sec per column.
    float head_y = mod(u_time * col_speed * 6.0 + col_offset * rows, rows);
    float dist_from_head = head_y - grid.y;
    float dist_wrapped   = mod(dist_from_head, rows);

    // trail visibility: must be below head, within trail length
    float fade_rate = max(u_param3, 0.01);
    float in_trail  = step(0.0, dist_from_head + 0.0001) * step(dist_wrapped, trail_len);
    float bright    = exp(-dist_wrapped * fade_rate) * in_trail * col_active;

    // ---- glyph rendering: 5x7 bitmap inside the cell ----
    vec2 cell_uv = fract(px / cell);
    // small margin so glyph doesn't touch cell edges (gives air between chars)
    cell_uv = (cell_uv - 0.5) * 1.10 + 0.5;
    vec2 sub = floor(cell_uv * vec2(5.0, 7.0));
    float in_bounds = step(0.0, sub.x) * step(0.0, sub.y)
                    * step(sub.x, 4.0) * step(sub.y, 6.0);

    // glyph id mutates at glyph_rate Hz; trail cells mutate slower so the
    // head locks-in characters as it passes (more film-accurate).
    float glyph_rate = u_param1;
    float t_quant_head  = floor(u_time * glyph_rate);
    float t_quant_trail = floor(u_time * glyph_rate * 0.15);
    float at_head = step(dist_wrapped, 0.5);
    float t_quant = mix(t_quant_trail, t_quant_head, at_head);
    float glyph_id = hash21(grid + vec2(t_quant, 0.0));

    // sub-pixel on/off — biased toward ~45% fill so glyphs look like text
    float pixel_h = hash11(glyph_id * 73.0 + sub.x * 7.0 + sub.y * 31.0);
    float pixel_on = step(0.55, pixel_h) * in_bounds;

    // ---- color: locked Matrix green ----
    vec3 matrix_green = vec3(0.10, 1.00, 0.30);
    // small tunable tint shift (toward cyan if user wants)
    vec3 tint = mix(matrix_green, vec3(0.10, 0.95, 0.55), u_param2);

    vec3 trail_color = tint * bright;
    // head is desaturated toward white for that signature bright-leading-cell
    vec3 head_color  = mix(tint, vec3(0.9, 1.0, 0.9), 0.75);
    vec3 color = mix(trail_color, head_color, at_head * col_active);

    // mask by glyph bitmap
    color *= pixel_on;

    // mild green ambient so empty cells in active columns aren't pure black
    color += tint * 0.012 * col_active * in_trail * pixel_on * (1.0 - at_head);

    gl_FragColor = vec4(color, 1.0);
}
