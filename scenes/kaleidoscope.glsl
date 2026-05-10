// u_param0  segments      (3..12) — fold count (floored to int)
// u_param1  rotate_speed  (-2..2) — rotation speed multiplier
// u_param2  zoom          (0.5..4) — radial zoom [bass]
// u_param3  hue           (0..1)  — base hue
// u_param4  warp          (0..2)  — distortion amount [lomid]
//
// 6-fold (or N-fold) kaleidoscope: fold UV into a wedge, sample a
// trig pattern inside. Bass drives zoom, lomid drives warp.
void main() {
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;
    float beat_time = u_time * bpm / 60.0;

    float segs = floor(u_param0);
    float speed = u_param1;
    float zoom = u_param2;
    float hue = u_param3;
    float warp = u_param4;

    // Center and convert to polar
    vec2 uv = v_uv - 0.5;
    uv *= zoom;
    float angle = atan(uv.y, uv.x);
    float radius = length(uv);

    // Rotate with time
    angle += beat_time * speed * 0.1;

    // Fold into one wedge [0 .. pi/segs]
    float wedge = 3.14159 / segs;
    angle = mod(angle, 2.0 * wedge);
    if (angle > wedge) angle = 2.0 * wedge - angle;

    // Reconstruct coords in wedge space
    vec2 p = vec2(cos(angle), sin(angle)) * radius;

    // Warp with lomid audio
    p += warp * 0.1 * vec2(sin(p.y * 4.0 + beat_time), cos(p.x * 4.0 + beat_time));

    // Trig pattern inside the wedge
    float v = sin(p.x * 8.0) * cos(p.y * 8.0) + 0.5 * sin(radius * 12.0 - beat_time * 2.0);
    v = 0.5 + 0.5 * v;

    // Slow BPM-locked hue drift so palette never sits static.
    float hue_drift = beat_time / 32.0;
    vec3 col = 0.5 + 0.5 * cos(6.2831 * (hue + hue_drift + v * 0.3 + vec3(0.0, 0.33, 0.66)));
    gl_FragColor = vec4(col, 1.0);
}
