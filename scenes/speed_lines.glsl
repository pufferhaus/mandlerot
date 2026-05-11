// Anime speed lines: radial black streaks from the screen center; density
// and length jump on beat. Optional inversion for "panel pop" look.
//
// u_param0 line_count    32..256  number of radial slots (160)
// u_param1 length        0..1     line length fraction (0.6)  [bass +]
// u_param2 width         0..1     per-line thickness
// u_param3 center_clear  0..0.5   radius of clear circle at center
// u_param4 invert        0..1     0 = black on white, 1 = white on black
// u_param5 rotation_rate 0..2     slow rotation of the line pattern
// u_param6 noise_jitter  0..1     per-line length jitter
// u_param7 beat_punch    0..1     extra length on beat trigger

float h(float x){ return fract(sin(x * 12.9898) * 43758.5453); }

void main() {
    vec2 uv = v_uv - 0.5;
    uv.x *= u_resolution.x / u_resolution.y;

    float r = length(uv);
    float a = atan(uv.y, uv.x);

    // rotate slowly so streaks don't feel frozen
    a += u_time * u_param5 * 0.3;

    float n_slots = max(u_param0, 8.0);
    float slot = floor((a / 6.2831 + 0.5) * n_slots);
    float local = fract((a / 6.2831 + 0.5) * n_slots) - 0.5;

    // per-slot random length + presence
    float slot_h = h(slot * 0.137);
    float length_jit = mix(1.0, slot_h, u_param6);
    float beat_kick = u_param7 * u_beat * 0.4;
    float len = u_param1 * length_jit + beat_kick;
    float r_min = u_param3;
    float r_max = r_min + len;

    // a line exists at this slot if its randomness exceeds an audio-modulated threshold
    float present = step(0.4 - u_audio.x * 0.3, slot_h);

    // line mask: in radial band AND inside angular thickness
    float in_radius = step(r_min, r) * step(r, r_max);
    float thickness = u_param2 * (0.5 + 0.5 * (1.0 - r));
    float in_arc = step(abs(local), thickness * 0.5);

    // taper line opacity toward both ends so they fade smoothly
    float taper = smoothstep(r_min, r_min + 0.05, r) * smoothstep(r_max, r_max - 0.1, r);
    float line = in_radius * in_arc * taper * present;

    vec3 bg = mix(vec3(1.0), vec3(0.02), u_param4);
    vec3 fg = mix(vec3(0.02), vec3(1.0), u_param4);
    vec3 col = mix(bg, fg, line);

    gl_FragColor = vec4(col, 1.0);
}
