// u_param0  bars      (8..64)    — number of bars (integer-ish)
// u_param1  hue_low   (0..1)     — bottom color hue
// u_param2  hue_high  (0..1)     — top color hue
// u_param3  smoothing (0..1)     — fall-off via u_prev sampling
// u_param4  spacing   (0..0.3)   — gap fraction between bars
//
// Classic vertical EQ bars; each bar reads a blended audio band.
void main() {
    float bars     = u_param0;
    float hue_low  = u_param1;
    float hue_high = u_param2;
    float smoothing = u_param3;
    float spacing   = u_param4;

    vec2 uv = v_uv;

    // Which bar are we in? t = normalized position along bar axis.
    float bar_idx = floor(uv.x * bars);
    float t = (bar_idx + 0.5) / bars; // 0..1 across all bars

    // Fraction within this bar cell (for spacing gap)
    float bar_frac = fract(uv.x * bars);
    float gap_half = spacing * 0.5;
    float in_bar = step(gap_half, bar_frac) * step(bar_frac, 1.0 - gap_half);

    // Blend audio bands based on bar position
    float bass   = u_audio.x;
    float lomid  = u_audio.y;
    float himid  = u_audio.z;
    float treble = u_audio.w;

    float level;
    if (t < 0.333) {
        level = mix(bass, lomid, t / 0.333);
    } else if (t < 0.667) {
        level = mix(lomid, himid, (t - 0.333) / 0.334);
    } else {
        level = mix(himid, treble, (t - 0.667) / 0.333);
    }

    // Smooth via prev frame: read the prev pixel's green channel as proxy height
    float prev_level = texture2D(u_prev, vec2(uv.x, 0.5)).g;
    level = mix(level, prev_level, smoothing);

    // Bar height test
    float bar_on = step(uv.y, level) * in_bar;

    // Color gradient bottom (hue_low) to top (hue_high)
    float hue = mix(hue_low, hue_high, uv.y);
    vec3 col = 0.5 + 0.5 * cos(6.2831 * (hue + vec3(0.0, 0.33, 0.66)));

    gl_FragColor = vec4(col * bar_on, 1.0);
}
