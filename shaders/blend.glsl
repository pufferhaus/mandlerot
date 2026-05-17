#version 100
precision mediump float;
uniform sampler2D u_layer_a;
uniform sampler2D u_layer_b;
uniform float u_xfade;
uniform int u_blend_mode;
uniform int u_key_enabled;     // 0 = off, 1 = on
uniform vec3 u_key_color;      // linear RGB
uniform float u_key_luma;      // threshold (Rec. 601)
uniform float u_key_soft;      // soft-edge half-width
uniform int u_key_spill;       // 0 = off, 1 = subtract chroma component
varying vec2 v_uv;

// All modes follow the same shape: `mix(a, OP(a,b), u_xfade)`. xfade=0 → A,
// xfade=1 → full blend. Keeping the wrapper uniform across modes lets the
// crossfade slider behave the same in every mode.

// Per-channel Soft Light (Photoshop simplified). Helper so we can compose
// the result component-wise via step()/mix() without an `if` ladder.
vec3 soft_light(vec3 a, vec3 b) {
    vec3 lo = 2.0 * a * b + a * a * (1.0 - 2.0 * b);
    vec3 hi = sqrt(a) * (2.0 * b - 1.0) + 2.0 * a * (1.0 - b);
    return mix(lo, hi, step(0.5, b));
}

// RGB ↔ HSL. Lifted from the standard CSS/Photoshop formulation. Used only
// by the four HSL-family modes; `if (mx != mn)` short-circuits the grayscale
// case so we never divide by zero.
vec3 rgb2hsl(vec3 c) {
    float mx = max(max(c.r, c.g), c.b);
    float mn = min(min(c.r, c.g), c.b);
    float l = (mx + mn) * 0.5;
    float h = 0.0;
    float s = 0.0;
    if (mx != mn) {
        float d = mx - mn;
        s = l > 0.5 ? d / (2.0 - mx - mn) : d / (mx + mn);
        if (mx == c.r)      h = (c.g - c.b) / d + (c.g < c.b ? 6.0 : 0.0);
        else if (mx == c.g) h = (c.b - c.r) / d + 2.0;
        else                h = (c.r - c.g) / d + 4.0;
        h /= 6.0;
    }
    return vec3(h, s, l);
}

float hue2rgb(float p, float q, float t) {
    if (t < 0.0) t += 1.0;
    if (t > 1.0) t -= 1.0;
    if (t < 1.0 / 6.0) return p + (q - p) * 6.0 * t;
    if (t < 1.0 / 2.0) return q;
    if (t < 2.0 / 3.0) return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    return p;
}

vec3 hsl2rgb(vec3 c) {
    float h = c.x;
    float s = c.y;
    float l = c.z;
    if (s == 0.0) return vec3(l);
    float q = l < 0.5 ? l * (1.0 + s) : l + s - l * s;
    float p = 2.0 * l - q;
    return vec3(
        hue2rgb(p, q, h + 1.0 / 3.0),
        hue2rgb(p, q, h),
        hue2rgb(p, q, h - 1.0 / 3.0)
    );
}

void main() {
    vec4 a = texture2D(u_layer_a, v_uv);
    vec4 b = texture2D(u_layer_b, v_uv);
    vec4 op;
    if      (u_blend_mode == 0)  op = b;                              // Mix (mix to B)
    else if (u_blend_mode == 1)  op = a + b;                          // Add
    else if (u_blend_mode == 2)  op = a * b;                          // Multiply
    else if (u_blend_mode == 3)  op = 1.0 - (1.0 - a) * (1.0 - b);    // Screen
    else if (u_blend_mode == 4)  op = abs(a - b);                     // Difference
    else if (u_blend_mode == 5)  op = mix(2.0 * a * b,                // Overlay
                                          1.0 - 2.0 * (1.0 - a) * (1.0 - b),
                                          step(0.5, b));
    else if (u_blend_mode == 6)  op = mix(2.0 * a * b,                // HardLight (Overlay w/ A,B swapped)
                                          1.0 - 2.0 * (1.0 - a) * (1.0 - b),
                                          step(0.5, a));
    else if (u_blend_mode == 7)  op = max(a, b);                      // Lighten
    else if (u_blend_mode == 8)  op = min(a, b);                      // Darken
    else if (u_blend_mode == 9)  op = a + b - 2.0 * a * b;            // Exclusion
    else if (u_blend_mode == 10) op = clamp(b - a, 0.0, 1.0);         // Subtract
    else if (u_blend_mode == 11) op = clamp(a + b - 1.0, 0.0, 1.0);   // LinearBurn
    else if (u_blend_mode == 12) op = vec4(soft_light(a.rgb, b.rgb), a.a); // SoftLight
    else if (u_blend_mode == 13) op = vec4(clamp(b.rgb / max(vec3(1.0) - a.rgb, vec3(1e-4)),
                                                 vec3(0.0), vec3(1.0)),     // ColorDodge
                                           a.a);
    else if (u_blend_mode == 14) op = vec4(vec3(1.0) - clamp((vec3(1.0) - b.rgb) / max(a.rgb, vec3(1e-4)),
                                                             vec3(0.0), vec3(1.0)),    // ColorBurn
                                           a.a);
    else if (u_blend_mode == 15) {
        // Hue: result's H from A, S+L from B.
        vec3 ha = rgb2hsl(a.rgb);
        vec3 hb = rgb2hsl(b.rgb);
        op = vec4(hsl2rgb(vec3(ha.x, hb.y, hb.z)), a.a);
    }
    else if (u_blend_mode == 16) {
        // Saturation: S from A, H+L from B.
        vec3 ha = rgb2hsl(a.rgb);
        vec3 hb = rgb2hsl(b.rgb);
        op = vec4(hsl2rgb(vec3(hb.x, ha.y, hb.z)), a.a);
    }
    else if (u_blend_mode == 17) {
        // Color: H+S from A, L from B.
        vec3 ha = rgb2hsl(a.rgb);
        vec3 hb = rgb2hsl(b.rgb);
        op = vec4(hsl2rgb(vec3(ha.x, ha.y, hb.z)), a.a);
    }
    else if (u_blend_mode == 18) {
        // Luminosity: L from A, H+S from B.
        vec3 ha = rgb2hsl(a.rgb);
        vec3 hb = rgb2hsl(b.rgb);
        op = vec4(hsl2rgb(vec3(hb.x, hb.y, ha.z)), a.a);
    }
    else                         op = b;                              // safe fallback
    vec4 mixed = mix(a, op, u_xfade);
    if (u_key_enabled == 1) {
        float l = dot(mixed.rgb, vec3(0.299, 0.587, 0.114));
        float t0 = u_key_luma - u_key_soft;
        float t1 = u_key_luma + max(u_key_soft, 1e-4);
        // factor = 1.0 when fully background, 0.0 when fully foreground.
        float factor = 1.0 - smoothstep(t0, t1, l);
        vec3 fg = mixed.rgb;
        if (u_key_spill == 1) {
            // Subtract the key-color projection on a per-channel basis so
            // edges don't carry the key tint downstream. Clamped to 0 to
            // avoid going negative on saturated foreground hits.
            float proj = dot(fg, u_key_color);
            fg = clamp(fg - u_key_color * proj * 0.5, 0.0, 1.0);
        }
        vec3 col = mix(fg, u_key_color, factor);
        gl_FragColor = vec4(col, mixed.a);
    } else {
        gl_FragColor = mixed;
    }
}
