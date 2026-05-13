// Per-pixel hashed grain. Modulated by luma so highlights/shadows stay clean.
// p0 amount    — 0..0.5 noise gain
// p1 luma_lock — 0 = grain everywhere, 1 = grain only in mid-tones
float hash21(vec2 p) {
    // Cheap fract(sin(dot)) hash. Plenty random for grain at 30 fps.
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

void main() {
    vec4 c = texture2D(u_input, v_uv);
    float amount    = u_param0;
    float luma_lock = u_param1;
    // Seed each frame so the grain animates instead of looking like a stuck
    // dirt pattern. fract() keeps the seed bounded.
    vec2 seed = v_uv * u_resolution + fract(u_time * 60.0) * 13.0;
    float n = hash21(seed) - 0.5;
    float luma = dot(c.rgb, vec3(0.299, 0.587, 0.114));
    // Bell-curve weight: 1.0 at luma=0.5, falls to (1-luma_lock) at the ends.
    float w = mix(1.0, 1.0 - 4.0 * (luma - 0.5) * (luma - 0.5), luma_lock);
    gl_FragColor = vec4(c.rgb + n * amount * w, c.a);
}
