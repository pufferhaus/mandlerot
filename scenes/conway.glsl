// Cellular automata layer. Each pixel is a "cell". The red channel of
// `u_prev` carries cell life (0 dead, 1 live). On every frame we sample
// the eight neighbours via 1-pixel UV offsets, apply Conway's rules, and
// inject a tiny amount of seed/regen noise so the field never goes
// permanently dark.
float hash21(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

void main() {
    vec2 px = 1.0 / u_resolution;
    // 8-neighbour life sum from the previous frame's red channel.
    float n = 0.0;
    n += texture2D(u_prev, v_uv + vec2(-px.x, -px.y)).r;
    n += texture2D(u_prev, v_uv + vec2( 0.0 , -px.y)).r;
    n += texture2D(u_prev, v_uv + vec2( px.x, -px.y)).r;
    n += texture2D(u_prev, v_uv + vec2(-px.x,  0.0 )).r;
    n += texture2D(u_prev, v_uv + vec2( px.x,  0.0 )).r;
    n += texture2D(u_prev, v_uv + vec2(-px.x,  px.y)).r;
    n += texture2D(u_prev, v_uv + vec2( 0.0 ,  px.y)).r;
    n += texture2D(u_prev, v_uv + vec2( px.x,  px.y)).r;

    float self_alive = texture2D(u_prev, v_uv).r;
    // Discrete neighbour count via floor + step thresholds.
    float n_int = floor(n + 0.5);
    float born = step(2.5, n_int) * step(n_int, 3.5); // n == 3
    float survive = step(1.5, n_int) * step(n_int, 3.5); // n in {2, 3}
    float next_alive = max(
        step(0.5, self_alive) * survive,        // live + (2 or 3) -> live
        (1.0 - step(0.5, self_alive)) * born    // dead + 3        -> live
    );

    // Trail: dying cells fade out instead of snapping to black, so the
    // viewer perceives motion. `u_param3` = trail (0..0.95).
    float trail_value = self_alive * u_param3;
    float life = max(next_alive, trail_value);

    // Seed phase + regen noise. During the first second we lay down dense
    // initial life at the user-supplied density. After that, a tiny amount
    // of randomness is injected each frame to keep the field dynamic.
    float t_seed = step(u_time, 1.0);
    float seed_density = u_param0;
    float regen = u_param1;
    float r = hash21(v_uv * u_resolution + floor(u_time * 60.0));
    float seed = step(1.0 - seed_density, r) * t_seed;
    float regen_hit = step(1.0 - regen, r) * (1.0 - t_seed);
    life = max(life, max(seed, regen_hit));

    // Colour the cells. Red channel carries the raw life value so the next
    // frame can recover it via texture2D(u_prev, ...).r; green/blue carry the
    // hue tint so the visible image is colourful, not just red.
    float hue = u_param2;
    vec3 tint = 0.5 + 0.5 * cos(6.2831 * (hue + vec3(0.0, 0.33, 0.66)));
    // Force the red channel to equal `life` exactly so the CA state survives
    // the round-trip through the FBO. Tint the green/blue for visuals.
    vec3 color = vec3(life, tint.g * life, tint.b * life);

    gl_FragColor = vec4(color, 1.0);
}
