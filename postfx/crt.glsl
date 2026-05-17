// CRT overlay: scanlines, barrel curvature, aperture mask, corner darken,
// asymmetric phosphor decay. All five effects in one pass for cache locality.
// Prelude provides u_input, u_prev, u_resolution, v_uv, u_param0..u_param7.

vec2 barrel(vec2 uv, float amount) {
    vec2 d = uv - 0.5;
    float r2 = dot(d, d);
    return uv + d * r2 * amount;
}

void main() {
    float scan_strength     = u_param0;
    float curvature         = u_param1;
    float aperture_strength = u_param2;
    float corner_darken     = u_param3;
    float phosphor_decay    = u_param4;

    vec2 uv = barrel(v_uv, curvature);

    // Off-screen → black. Barrel warp can push UV outside [0,1].
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0);
        return;
    }

    vec3 col = texture2D(u_input, uv).rgb;

    // Phosphor decay: asymmetric max-blend with last frame's chain output.
    // Bright pixels persist + fade; dark pixels update immediately. u_prev
    // sampled at v_uv (screen space) — last frame's output was already
    // barrel-warped, so re-warping would double-distort.
    if (phosphor_decay > 0.0) {
        vec3 prev = texture2D(u_prev, v_uv).rgb;
        col = max(col, prev * phosphor_decay);
    }

    // Scanlines: cos period over physical row count.
    float row = uv.y * u_resolution.y;
    float scan = 1.0 - scan_strength * 0.5 * (1.0 - cos(row * 6.2831853));
    col *= scan;

    // Aperture mask: 3-column RGB tint, repeats every 3 physical columns.
    float col_idx = mod(floor(uv.x * u_resolution.x), 3.0);
    vec3 mask = vec3(
        col_idx < 0.5 ? 1.0 : 1.0 - aperture_strength,
        (col_idx >= 0.5 && col_idx < 1.5) ? 1.0 : 1.0 - aperture_strength,
        col_idx >= 1.5 ? 1.0 : 1.0 - aperture_strength
    );
    col *= mask;

    // Corner darken — pinned to the barrel warp, separate from generic Vignette.
    vec2 d = v_uv - 0.5;
    float corner = 1.0 - corner_darken * smoothstep(0.35, 0.7, length(d));
    col *= corner;

    gl_FragColor = vec4(col, 1.0);
}
