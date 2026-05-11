// Faux BIOS POST scroll. Lines of hashed text scroll upward; every so
// often a line ends in [OK] (mostly) or [FAIL] (rarely). Audio modulates
// scroll speed. Single line height = one text row. 5x7 cell glyphs.
//
// u_param0 scroll_speed  0.5..6   lines per second (2.0)
// u_param1 ok_fail_rate  0..1     fraction of lines that get a status tag
// u_param2 hue           0..1     amber/green palette
// u_param3 cell_w        5..10    pixel width of one cell (6)
// u_param4 cell_h        7..14    pixel height of one cell (10)
// u_param5 cursor_blink  0..2     cursor blink Hz (1.5)
// u_param6 audio_speed   0..1     bass kicks scroll speed
// u_param7 brightness    0..2     global gain

float h(vec2 p){ return fract(sin(dot(p, vec2(12.9898,78.233))) * 43758.5453); }
float h1(float x){ return fract(sin(x * 12.9898) * 43758.5453); }

float glyph_pixel(float id, vec2 sub) {
    float r = h(vec2(id * 13.0 + sub.x * 5.0, sub.y));
    return step(0.55, r);
}

void main() {
    float cw = max(u_param3, 4.0);
    float ch = max(u_param4, 6.0);
    vec2 px = v_uv * u_resolution;
    vec2 cell = floor(px / vec2(cw, ch));
    vec2 cell_uv = fract(px / vec2(cw, ch));
    vec2 sub = floor(cell_uv * vec2(5.0, 7.0));
    float in_bounds = step(0.0, sub.x) * step(0.0, sub.y)
                    * step(sub.x, 4.0) * step(sub.y, 6.0);

    // scroll the rows upward over time
    float speed = u_param0 * (1.0 + u_param6 * u_audio.x);
    float line_idx = cell.y + floor(u_time * speed);

    // per-line presence + content: each line has a max char count
    float line_len = 30.0 + h1(line_idx * 0.13) * 30.0;
    float in_text = step(cell.x, line_len);

    // glyph id per cell — biased toward letter/number range
    float glyph_id = floor(h(vec2(cell.x, line_idx)) * 26.0);
    float on = glyph_pixel(glyph_id, sub) * in_text * in_bounds;

    // append "[OK]" or "[FAIL]" near end of some lines
    float tail_start = line_len - 6.0;
    float status_seed = h1(line_idx);
    float wants_status = step(1.0 - u_param1, status_seed);
    float is_fail = step(0.92, h1(line_idx + 7.0));
    if (wants_status > 0.5 && cell.x > tail_start && cell.x < tail_start + 6.0) {
        // simulate a tag by forcing dense pixels there
        float tag_id = floor(h(vec2(cell.x, line_idx + 1.0)) * 6.0);
        on = step(0.4, h(sub + vec2(tag_id, 0.0))) * in_bounds;
    }

    // palette
    vec3 amber = vec3(1.0, 0.7, 0.1);
    vec3 green = vec3(0.2, 1.0, 0.4);
    vec3 red = vec3(1.0, 0.2, 0.2);
    vec3 base_tint = mix(amber, green, u_param2);
    vec3 tag_tint = mix(base_tint, red, is_fail * wants_status);
    bool in_tag = (wants_status > 0.5) && (cell.x > tail_start) && (cell.x < tail_start + 6.0);
    vec3 tint = in_tag ? tag_tint : base_tint;

    vec3 col = tint * on;

    // blinking cursor at the bottom-left of the visible region
    float cursor_row = 0.0; // bottom-most cell row
    float cursor_col = mod(floor(u_time * 5.0) * 1.0, 30.0); // wandering
    if (cell.y < 0.5 && abs(cell.x - cursor_col) < 0.5) {
        float blink = step(0.5, fract(u_time * u_param5));
        col = mix(col, tint, blink);
    }

    // mild scanline
    col *= 0.85 + 0.15 * sin(v_uv.y * u_resolution.y * 3.1415);
    col *= u_param7;
    gl_FragColor = vec4(col, 1.0);
}
