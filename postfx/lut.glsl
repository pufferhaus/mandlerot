// LUT colour-grade post-FX pass.
// LUT is a 256x16 RGBA strip: 16 slices of 16x16 each.
//   slice index = blue, x-within-slice = red, y = green.
// Manually interpolate the B axis (NEAREST sampling on u_lut).

uniform sampler2D u_lut;

vec3 sample_lut(vec3 c) {
    float b  = clamp(c.b, 0.0, 1.0) * 15.0;
    float lo = floor(b);
    float hi = min(lo + 1.0, 15.0);
    float bf = b - lo;
    float r  = clamp(c.r, 0.0, 1.0) * 15.0;
    float g  = clamp(c.g, 0.0, 1.0) * 15.0;
    vec2 uv_lo = vec2((r + lo * 16.0 + 0.5) / 256.0, (g + 0.5) / 16.0);
    vec2 uv_hi = vec2((r + hi * 16.0 + 0.5) / 256.0, (g + 0.5) / 16.0);
    return mix(texture2D(u_lut, uv_lo).rgb,
               texture2D(u_lut, uv_hi).rgb, bf);
}

void main() {
    vec3 src = texture2D(u_prev, v_uv).rgb;
    gl_FragColor = vec4(mix(src, sample_lut(src), u_param1), 1.0);
}
