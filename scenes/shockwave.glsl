// u_param0  ring_count      (2..8)         — number of simultaneous rings
// u_param1  ring_thickness  (0.005..0.05)  — ring line width
// u_param2  expand_speed    (0.1..2.0)     — outward expansion rate [bass 0.5]
// u_param3  hue             (0..1)         — ring color hue
// u_param4  fade_dist       (0.3..1.0)     — radius at which ring vanishes
//
// Concentric expanding rings on beat; staggered via modular phase offsets.
void main() {
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;

    float ring_count     = u_param0;
    float ring_thickness = u_param1;
    float expand_speed   = u_param2;
    float hue            = u_param3;
    float fade_dist      = u_param4;

    // Aspect-corrected distance from center
    vec2 c = (v_uv - 0.5) * vec2(u_resolution.x / u_resolution.y, 1.0);
    float r = length(c);

    float period = 1.0; // one ring spawns per beat period
    float beat_phase = u_time * bpm / 60.0;

    float glow = 0.0;

    // Loop hardcoded at 8; mask by ring_count
    for (int i = 0; i < 8; i++) {
        float fi = float(i);
        if (fi >= ring_count) { break; }
        float offset = fi / ring_count;
        float phase = mod(beat_phase - offset, 1.0); // 0..1, where 0=just spawned
        float ring_r = phase * expand_speed;
        float fade = 1.0 - smoothstep(0.0, fade_dist, ring_r);
        float dist = abs(r - ring_r);
        glow += fade * smoothstep(ring_thickness, 0.0, dist);
    }

    glow = clamp(glow, 0.0, 1.0);
    float hue_drift = beat_phase / 32.0;
    vec3 col = 0.5 + 0.5 * cos(6.2831 * (hue + hue_drift + vec3(0.0, 0.33, 0.66)));

    gl_FragColor = vec4(col * glow, 1.0);
}
