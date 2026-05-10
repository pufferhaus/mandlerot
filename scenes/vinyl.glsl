// u_param0  groove_density  (50..400, 200) — grooves per unit radius (integer-ish)
// u_param1  rot_speed       (-2..2, 0.3)   — rotation speed
// u_param2  wobble_amount   (0..0.05, 0.005) — bass wobble radius [bass 1.0]
// u_param3  hue             (0..1, 0.05)   — label hue (default red)
// u_param4  label_size      (0.05..0.3, 0.12) — center label radius
//
// Vinyl record: concentric grooves, rotating, with bass-driven wobble.
// Center label has a distinct hue; grooves reflect light as concentric bands.
void main() {
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;

    float groove_density = floor(u_param0 + 0.5);
    float rot_speed      = u_param1;
    float wobble         = u_param2;
    float label_hue      = u_param3;
    float label_size     = u_param4;

    vec2 c = v_uv - 0.5;
    float radius = length(c);
    float angle  = atan(c.y, c.x);

    // Rotation
    angle += u_time * rot_speed;

    // Bass wobble: record "skips" on kick
    float wobble_r = radius + sin(angle * 4.0 + u_time * 5.0) * wobble;

    // Groove pattern: alternating bands
    float groove = mod(wobble_r * groove_density, 1.0);
    float groove_bright = 0.5 + 0.5 * sin(groove * 6.2831);

    // Groove shading: darker at edges, slight angle-based sheen
    float sheen = 0.7 + 0.3 * sin(angle * 2.0 + u_time * 0.5);
    float record_col = groove_bright * sheen * (1.0 - smoothstep(0.45, 0.5, radius));

    // Vinyl color (very dark, slightly blue-grey)
    vec3 vinyl = vec3(0.05, 0.05, 0.07) + record_col * vec3(0.3, 0.3, 0.35);

    // Center label
    float in_label = smoothstep(label_size + 0.005, label_size, radius);
    vec3 label_base = 0.5 + 0.5 * cos(6.2831 * (label_hue + vec3(0.0, 0.33, 0.66)));
    // Label spin pattern: concentric rings
    float label_rings = 0.6 + 0.4 * sin(radius * 80.0 + angle * 2.0 + u_time);
    vec3 label_col = label_base * label_rings;

    vec3 col = mix(vinyl, label_col, in_label);

    // Fade to black beyond record edge
    col *= smoothstep(0.51, 0.48, radius);

    col = clamp(col, 0.0, 1.0);
    gl_FragColor = vec4(col, 1.0);
}
