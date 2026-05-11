// Safe-scene test pattern. Shown when PANIC is engaged and as the fallback
// when a user scene fails to compile or auto-disables after repeated faults.
// Modeled on SMPTE 75% color bars + a reverse strip + a black/white step ramp.
// A small blinking square in the lower-right confirms u_time is alive.
void main() {
    vec2 uv = v_uv;
    vec3 col;

    if (uv.y > 0.33) {
        // Main bars: 7 columns across the top two-thirds.
        float idx = floor(uv.x * 7.0);
        if      (idx < 0.5) col = vec3(0.75, 0.75, 0.75); // white
        else if (idx < 1.5) col = vec3(0.75, 0.75, 0.00); // yellow
        else if (idx < 2.5) col = vec3(0.00, 0.75, 0.75); // cyan
        else if (idx < 3.5) col = vec3(0.00, 0.75, 0.00); // green
        else if (idx < 4.5) col = vec3(0.75, 0.00, 0.75); // magenta
        else if (idx < 5.5) col = vec3(0.75, 0.00, 0.00); // red
        else                col = vec3(0.00, 0.00, 0.75); // blue
    } else if (uv.y > 0.22) {
        // Reverse strip: blue, black, magenta, black, cyan, black, white.
        float idx = floor(uv.x * 7.0);
        if      (idx < 0.5) col = vec3(0.00, 0.00, 0.75);
        else if (idx < 1.5) col = vec3(0.00);
        else if (idx < 2.5) col = vec3(0.75, 0.00, 0.75);
        else if (idx < 3.5) col = vec3(0.00);
        else if (idx < 4.5) col = vec3(0.00, 0.75, 0.75);
        else if (idx < 5.5) col = vec3(0.00);
        else                col = vec3(0.75);
    } else {
        // Greyscale step ramp 0..1 in 8 stops.
        float step8 = floor(uv.x * 8.0) / 7.0;
        col = vec3(step8);
    }

    // Liveness indicator: 1Hz blinking square in the lower-right corner.
    vec2 d = abs(uv - vec2(0.93, 0.07)) * vec2(20.0, 12.0);
    if (max(d.x, d.y) < 1.0) {
        col = vec3(step(0.5, fract(u_time)));
    }

    gl_FragColor = vec4(col, 1.0);
}
