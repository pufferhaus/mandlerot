#version 100
precision mediump float;
uniform sampler2D u_layer_a;
uniform sampler2D u_layer_b;
uniform float u_xfade;
uniform int u_blend_mode;
varying vec2 v_uv;

void main() {
    vec4 a = texture2D(u_layer_a, v_uv);
    vec4 b = texture2D(u_layer_b, v_uv);
    vec4 mixed;
    if      (u_blend_mode == 0) mixed = mix(a, b, u_xfade);
    else if (u_blend_mode == 1) mixed = mix(a, a + b, u_xfade);
    else if (u_blend_mode == 2) mixed = mix(a, a * b, u_xfade);
    else if (u_blend_mode == 3) mixed = mix(a, 1.0 - (1.0 - a) * (1.0 - b), u_xfade);
    else                        mixed = mix(a, abs(a - b), u_xfade);
    gl_FragColor = mixed;
}
