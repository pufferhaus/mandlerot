// Lorenz attractor — numerical integration with Gaussian splatting.
//
// Each pixel integrates the Lorenz system for 64 steps and accumulates
// brightness wherever a trajectory step lands nearby. A 200-step pre-roll
// burns off the initial transient; animation comes from a slow time-varying
// perturbation to the initial condition + hue cycling.
//
// Lorenz: dx/dt = σ(y−x), dy/dt = x(ρ−z)−y, dz/dt = xy−βz
//   σ=10, ρ=28, β=8/3
//
// Projection: Lorenz x → screen x, Lorenz z → screen y (gives double-lobe)
//   screen_x = pos.x / 40.0 + 0.5
//   screen_y = (pos.z - 20.0) / 30.0 + 0.5
//
// u_param0  speed   (0.1..5.0, default 1.0) — trajectory dt scale (also animates IC)
// u_param1  spot    (0.002..0.04, default 0.012) — Gaussian splat radius
// u_param2  hue     (0.0..1.0, default 0.35) — base tint hue

void main() {
    float sigma = 10.0;
    float rho   = 28.0;
    float beta  = 8.0 / 3.0;
    float dt    = 0.01 * u_param0;

    // Slowly-drifting initial condition so the trajectory evolves visually
    float t = u_time * u_param0 * 0.05;
    vec3 p = vec3(0.1 + 0.02 * sin(t), 0.03 * cos(t * 0.7), 0.0);

    // Pre-roll 200 steps to escape transient (fixed bound for GLES 1.00)
    for (int i = 0; i < 200; i++) {
        vec3 dp;
        dp.x = sigma * (p.y - p.x);
        dp.y = p.x * (rho - p.z) - p.y;
        dp.z = p.x * p.y - beta * p.z;
        p += dp * dt;
    }

    // Now accumulate brightness from 64 plotted steps
    // spot radius is u_param1, hue offset is u_param2
    float brightness = 0.0;
    float spot_r = u_param1;

    for (int i = 0; i < 64; i++) {
        vec3 dp;
        dp.x = sigma * (p.y - p.x);
        dp.y = p.x * (rho - p.z) - p.y;
        dp.z = p.x * p.y - beta * p.z;
        p += dp * dt;

        // Project: Lorenz x → screen x, Lorenz z → screen y (double-lobe view)
        float sx = p.x / 40.0 + 0.5;
        float sy = (p.z - 20.0) / 30.0 + 0.5;
        vec2 screen_pos = vec2(sx, sy);

        float d = length(screen_pos - v_uv);
        brightness += exp(-d * d / (2.0 * spot_r * spot_r));
    }

    brightness = clamp(brightness, 0.0, 1.0);

    // Hue cycling (slow drift + u_param2 offset)
    float hue = u_param2 + u_time * 0.04;
    vec3 tint = 0.5 + 0.5 * cos(6.28318 * (hue + vec3(0.0, 0.33, 0.66)));

    // Treble audio brightens the attractor slightly
    float audio_bright = 1.0 + u_audio.z * 0.3;

    vec3 col = tint * brightness * audio_bright;
    gl_FragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
