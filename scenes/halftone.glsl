// Ben-Day halftone dots: a grid of dots whose radius is driven by a
// continuous value field. Two-color palette. Audio modulates the field so
// dots breathe.
//
// u_param0 grid_size     6..40    pixel size of one cell (16)
// u_param1 angle         0..1     rotation of the dot grid (0..360deg)
// u_param2 contrast      0.5..3   sharpens the dot mask
// u_param3 hue           0..1     fg color (dot color)
// u_param4 bg_hue        0..1     bg color
// u_param5 wave_speed    0..2     animation speed of the field
// u_param6 wave_scale    0.5..4   field frequency
// u_param7 audio_pump    0..1     bass kicks dot size globally

float h(vec2 p){ return fract(sin(dot(p, vec2(12.9898,78.233))) * 43758.5453); }

void main() {
    vec2 px = v_uv * u_resolution;
    // rotate the dot grid by `angle` radians
    float a = u_param1 * 6.2831;
    mat2 R = mat2(cos(a), -sin(a), sin(a), cos(a));
    vec2 rpx = R * (px - u_resolution * 0.5);

    vec2 grid = floor(rpx / u_param0);
    vec2 cell_uv = fract(rpx / u_param0) - 0.5;

    // value field: sin + sin + audio bump
    vec2 nuv = (v_uv - 0.5) * u_param6;
    float t = u_time * u_param5;
    float val = 0.5 + 0.5 * sin(nuv.x * 4.0 + t)
              + 0.25 * sin(nuv.y * 5.0 - t * 0.7)
              + 0.25 * cos(length(nuv) * 6.0 - t * 0.5);
    val = clamp(val * 0.5, 0.0, 1.0);
    val += u_param7 * u_audio.x * 0.4;

    // dot radius proportional to value; max ~ half-cell
    float r = pow(clamp(val, 0.0, 1.0), u_param2) * 0.55;
    float d = length(cell_uv);
    // smooth edge so dots don't alias
    float mask = smoothstep(r + 0.04, r - 0.04, d);

    vec3 fg = 0.5 + 0.5 * cos(6.2831 * (u_param3 + vec3(0.0, 0.33, 0.66)));
    vec3 bg = 0.5 + 0.5 * cos(6.2831 * (u_param4 + vec3(0.0, 0.33, 0.66)));
    vec3 col = mix(bg, fg, mask);
    gl_FragColor = vec4(col, 1.0);
}
