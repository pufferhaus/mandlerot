// Video-feedback / echo layer. Each frame we sample the previous frame
// after a small rotate-zoom transformation and a brightness decay, then
// composite a fresh ring of colour on top. The recursive feedback loop
// produces an infinite-tunnel effect.
void main() {
    vec2 c = v_uv - 0.5;

    float angle = u_param0 * u_time;
    float zoom_in = 1.0 + u_param1; // bass-driven outward push
    mat2 rot = mat2(cos(angle), -sin(angle), sin(angle), cos(angle));
    vec2 prev_uv = (rot * c) / zoom_in + 0.5;

    // Decay both colour and alpha so the echo doesn't accumulate to white.
    vec3 echo = texture2D(u_prev, prev_uv).rgb * u_param2;

    // Fresh ring of colour stamped on top each frame. Hue cycles via param3.
    float r = length(c);
    float ring = smoothstep(0.30, 0.28, r) - smoothstep(0.32, 0.30, r);
    vec3 fresh = ring * (vec3(0.5) + 0.5 * cos(6.2831 * (u_param3 + vec3(0.0, 0.33, 0.66))));

    gl_FragColor = vec4(echo + fresh, 1.0);
}
