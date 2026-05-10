// Video-feedback / echo layer. Beat ring + BPM-locked hue rotation prevent
// convergence to all-black or all-white.
void main() {
    vec2 c = v_uv - 0.5;

    float angle = u_param0 * u_time;
    float zoom_in = 1.0 + u_param1;
    mat2 rot = mat2(cos(angle), -sin(angle), sin(angle), cos(angle));
    vec3 echo = texture2D(u_prev, (rot * c) / zoom_in + 0.5).rgb * u_param2;

    // BPM-locked hue rotation: one full cycle per 8 beats.
    // u_param3 is preserved as hue offset so user tweaks still shift palette.
    float bpm = mix(120.0, u_bpm, step(1.0, u_bpm));
    float hue = u_param3 + u_time * bpm / (60.0 * 8.0);

    // Beat ring: radius and brightness boosted on beats; bass modulates radius.
    float beat_bump = u_beat * 0.05;
    float bass_bump = u_audio.x * 0.03;
    float r = length(c);
    float ring_r = 0.30 + beat_bump + bass_bump;
    float ring = smoothstep(ring_r - 0.02, ring_r - 0.02 + 0.005, r)
               - smoothstep(ring_r,        ring_r + 0.005,         r);
    float brightness = 1.0 + 3.0 * u_beat;
    vec3 ring_col = brightness * ring * (vec3(0.5) + 0.5 * cos(6.2831 * (hue + vec3(0.0, 0.33, 0.66))));

    // On trigger: offset ring to a hashed position to break centred fixed point.
    float trig_gate = step(0.5, u_trigger);
    float bkt = floor(u_time * bpm / 60.0);
    vec2 trig_off = (vec2(fract(sin(bkt * 12.9898) * 43758.5453),
                          fract(sin(bkt * 78.233)  * 43758.5453)) - 0.5) * 0.3;
    vec2 ct = v_uv - 0.5 - trig_off * trig_gate;
    float rt = length(ct);
    float ring_t = smoothstep(ring_r - 0.02, ring_r - 0.02 + 0.005, rt)
                 - smoothstep(ring_r,        ring_r + 0.005,         rt);
    ring_col = mix(ring_col, brightness * ring_t
                   * (vec3(0.5) + 0.5 * cos(6.2831 * (hue + vec3(0.0, 0.33, 0.66)))),
                   trig_gate);

    gl_FragColor = vec4(echo + ring_col, 1.0);
}
