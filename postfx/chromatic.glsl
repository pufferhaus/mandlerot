// RGB split scaled by distance from screen centre. Cheap classic.
// p0 shift   — magnitude of the per-channel offset at the screen edge
// p1 falloff — exponent: 1 = linear, 2 = quadratic, etc. Higher = effect
//              confined to corners.
void main() {
    vec2 d = v_uv - 0.5;
    float r = length(d) * 2.0;          // 0 at centre, 1 at edge
    float k = pow(r, max(u_param1, 0.001));
    vec2 dir = (length(d) > 1e-4) ? normalize(d) : vec2(0.0);
    vec2 off = dir * u_param0 * k;
    float red   = texture2D(u_input, v_uv + off).r;
    float green = texture2D(u_input, v_uv).g;
    float blue  = texture2D(u_input, v_uv - off).b;
    float alpha = texture2D(u_input, v_uv).a;
    gl_FragColor = vec4(red, green, blue, alpha);
}
