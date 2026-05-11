// VHS tracking artifacts: chroma offset, rolling head-switch noise band,
// random dropout lines, intermittent color bleed. Reads as a beat-up
// cassette. Self-contained base pattern beneath the artifacts.
//
// u_param0 chroma_offset  0..1  U/V offset width (0.5)
// u_param1 head_band      0..1  speed of head-switch noise band (0.5)
// u_param2 dropout_rate   0..1  frequency of horizontal tears [treble +]
// u_param3 bleed          0..1  horizontal chroma bleed amount
// u_param4 hue            0..1  base hue
// u_param5 saturation     0..2  color saturation (1.0)
// u_param6 desync         0..1  occasional sync slip
// u_param7 brightness     0..2  global gain

float h21(vec2 p){ return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453); }

vec3 base(vec2 uv) {
    float h = u_param4 + 0.15 * sin(u_time * 0.3) + u_audio.x * 0.2;
    float v = uv.x + 0.3 * sin(uv.y * 6.0 + u_time);
    return 0.5 + 0.5 * cos(6.2831 * (h + v * 0.4 + vec3(0.0, 0.33, 0.66)));
}

// Convert RGB to a rough YUV so we can offset chroma vs luma.
vec3 rgb2yuv(vec3 c){
    float y = dot(c, vec3(0.299, 0.587, 0.114));
    float u = (c.b - y) * 0.493;
    float v = (c.r - y) * 0.877;
    return vec3(y, u, v);
}
vec3 yuv2rgb(vec3 c){
    float r = c.x + 1.140 * c.z;
    float g = c.x - 0.395 * c.y - 0.581 * c.z;
    float b = c.x + 2.032 * c.y;
    return vec3(r, g, b);
}

void main() {
    vec2 uv = v_uv;

    // desync: small horizontal jitter that shifts an entire scanline
    float line = floor(uv.y * u_resolution.y);
    float jitter = (h21(vec2(line, floor(u_time * 50.0))) - 0.5) * u_param6 * 0.04;
    uv.x = fract(uv.x + jitter);

    // sample luma at uv, chroma at uv + offset (so color smears right of luma)
    float off = u_param0 * (0.012 + 0.02 * u_audio.x);
    vec3 yuv_y = rgb2yuv(base(uv));
    vec3 yuv_c = rgb2yuv(base(uv + vec2(off, 0.0)));
    // Optional horizontal bleed by averaging a few neighbor samples on chroma.
    vec3 bleed1 = rgb2yuv(base(uv + vec2(off + 0.01, 0.0)));
    vec3 bleed2 = rgb2yuv(base(uv + vec2(off - 0.01, 0.0)));
    vec3 chroma = mix(yuv_c, (bleed1 + bleed2) * 0.5, u_param3);
    vec3 col = yuv2rgb(vec3(yuv_y.x, chroma.y * u_param5, chroma.z * u_param5));

    // head-switch noise band: a horizontal stripe rolling upward
    float band_y = fract(-u_time * (0.15 + u_param1 * 0.4));
    float band = smoothstep(0.04, 0.0, abs(uv.y - band_y));
    if (band > 0.0) {
        float snow = h21(uv * u_resolution.xy + u_time);
        col = mix(col, vec3(snow), band * 0.9);
    }

    // dropout tears: a few random scanlines get shifted hard or replaced
    float drop_seed = h21(vec2(line, floor(u_time * 8.0)));
    float drop = step(1.0 - u_param2 * 0.05, drop_seed);
    if (drop > 0.0) {
        col = mix(col, vec3(0.9, 0.9, 0.95), 0.6);
    }

    col *= u_param7;
    gl_FragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
