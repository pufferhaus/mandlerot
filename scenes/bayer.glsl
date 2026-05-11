// Bayer 4x4 ordered dither. A continuous grayscale field is computed from
// audio + slow noise, then thresholded against a 4x4 Bayer matrix to
// produce 1-bit output. Mac Plus aesthetic.
//
// u_param0 source       0..1     0 = synthetic field, 1 = u_prev (feedback)
// u_param1 brightness   0..2     gain into the threshold compare
// u_param2 wave_speed   0..2     animation rate of the synthetic field
// u_param3 wave_scale   0.5..4   frequency of the synthetic field
// u_param4 audio_pump   0..1     bass adds brightness globally
// u_param5 fg_hue       0..1     foreground color (white default)
// u_param6 bg_hue       0..1     background color (black default)
// u_param7 invert       0..1     0 = white-on-black, 1 = black-on-white

float bayer4(vec2 p) {
    // 4x4 Bayer matrix indexed by integer (x mod 4, y mod 4). Values
    // normalized to [0, 1) — there are 16 distinct thresholds.
    int x = int(mod(p.x, 4.0));
    int y = int(mod(p.y, 4.0));
    // hand-coded lookup: standard 4x4 ordered dither matrix
    float v;
    if      (x == 0 && y == 0) v =  0.0;
    else if (x == 2 && y == 0) v =  8.0;
    else if (x == 0 && y == 2) v = 10.0;
    else if (x == 2 && y == 2) v =  2.0;
    else if (x == 1 && y == 0) v = 12.0;
    else if (x == 3 && y == 0) v =  4.0;
    else if (x == 1 && y == 2) v =  6.0;
    else if (x == 3 && y == 2) v = 14.0;
    else if (x == 0 && y == 1) v =  3.0;
    else if (x == 2 && y == 1) v = 11.0;
    else if (x == 0 && y == 3) v =  9.0;
    else if (x == 2 && y == 3) v =  1.0;
    else if (x == 1 && y == 1) v = 15.0;
    else if (x == 3 && y == 1) v =  7.0;
    else if (x == 1 && y == 3) v =  5.0;
    else                       v = 13.0;
    return (v + 0.5) / 16.0;
}

float synth_field(vec2 uv) {
    float t = u_time * u_param2;
    float v = 0.5 + 0.25 * sin(uv.x * 5.0 * u_param3 + t)
                  + 0.25 * cos(uv.y * 4.0 * u_param3 - t * 0.7)
                  + 0.15 * sin(length(uv - 0.5) * 8.0 * u_param3 - t);
    return clamp(v, 0.0, 1.0);
}

void main() {
    vec2 uv = v_uv;
    float src_synth = synth_field(uv);
    float src_prev = dot(texture2D(u_prev, uv).rgb, vec3(0.299, 0.587, 0.114));
    float src = mix(src_synth, src_prev, u_param0);

    src *= u_param1;
    src += u_audio.x * u_param4 * 0.3;
    src = clamp(src, 0.0, 1.0);

    vec2 px = v_uv * u_resolution;
    float thr = bayer4(floor(px));
    float bit = step(thr, src);

    vec3 fg = 0.5 + 0.5 * cos(6.2831 * (u_param5 + vec3(0.0, 0.33, 0.66)));
    vec3 bg = 0.5 + 0.5 * cos(6.2831 * (u_param6 + vec3(0.0, 0.33, 0.66))) * 0.05;
    if (u_param7 > 0.5) {
        vec3 tmp = fg;
        fg = bg;
        bg = tmp;
    }
    vec3 col = mix(bg, fg, bit);
    gl_FragColor = vec4(col, 1.0);
}
