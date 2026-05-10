// u_param0  grid_speed  (0..2)      — scroll rate [bass 0.5]
// u_param1  sun_radius  (0.1..0.4)  — sun semicircle size [bass 0.2]
// u_param2  hue_grid    (0..1)      — grid color hue (pink/magenta ~0.85)
// u_param3  hue_sun     (0..1)      — sun color hue (orange/red ~0.05)
// u_param4  grid_fade   (0.5..3.0)  — distance fade for far grid lines
//
// Outrun-style perspective grid with scrolling horizon, neon lines, and banded sun.
void main() {
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;

    float grid_speed = u_param0;
    float sun_radius = u_param1;
    float hue_grid   = u_param2;
    float hue_sun    = u_param3;
    float grid_fade  = u_param4;

    vec2 uv = v_uv; // 0..1
    float horizon = 0.5;

    vec3 col = vec3(0.0);

    // --- Sun (upper half) ---
    vec2 sun_center = vec2(0.5, horizon);
    float dist_sun = length(uv - sun_center);
    float in_sun = step(dist_sun, sun_radius) * step(horizon, uv.y);
    // Horizontal palette stripes (every ~0.03 screen units)
    float stripe = step(0.5, fract(uv.y * 18.0));
    float sun_mask = in_sun * stripe;
    vec3 sun_col = 0.5 + 0.5 * cos(6.2831 * (hue_sun + vec3(0.0, 0.33, 0.66)));
    // Bass pulsing: brightness boost
    sun_col *= 1.0 + u_audio.x * 0.4;
    col += sun_col * sun_mask;

    // --- Grid (lower half only) ---
    if (uv.y < horizon) {
        // Map screen Y to perspective Z: near = bottom, far = horizon
        float fy = horizon - uv.y;             // 0 at horizon, 0.5 at bottom
        float persp_z = 1.0 / (fy + 0.001);   // large near top, small at bottom
        float scroll = u_time * grid_speed;

        // Horizontal lines at integer Z steps, scrolled
        float hz = mod(persp_z + scroll * 2.0, 1.0);
        float h_line = step(0.93, hz);
        // Fade distant lines (small fy = near horizon = far away)
        float dist_fade = 1.0 - exp(-fy * grid_fade * 4.0);

        // Vertical lines perspective-corrected: map uv.x through persp
        float vx = (uv.x - 0.5) * persp_z * 0.3; // narrows at horizon
        float v_line = step(0.94, 1.0 - abs(fract(vx * 5.0 + 0.5) - 0.5) * 2.0);

        float grid_mask = clamp(h_line + v_line, 0.0, 1.0) * dist_fade;
        // Grid brightness boosted by bass
        float grid_bright = 1.0 + u_audio.x * 0.5;
        vec3 grid_col = 0.5 + 0.5 * cos(6.2831 * (hue_grid + vec3(0.0, 0.33, 0.66)));
        col += grid_col * grid_mask * grid_bright;
    }

    col = clamp(col, 0.0, 1.0);
    gl_FragColor = vec4(col, 1.0);
}
