// Conway CA: red channel carries cell state. Audio-driven reseed prevents
// convergence to still-lifes or empty board. Periodic full reset every
// wipe_beats beats clears stagnation and re-seeds from density.
float hash21(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

void main() {
    vec2 px = 1.0 / u_resolution;
    vec2 gp = floor(v_uv * u_resolution);

    // Periodic full wipe: zero the board every wipe_beats beats.
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;
    float beats = u_time * bpm / 60.0;
    float in_wipe = step(mod(beats, u_param4), 0.1);

    // 8-neighbour life sum from previous frame red channel.
    float n = 0.0;
    n += texture2D(u_prev, v_uv + vec2(-px.x,-px.y)).r + texture2D(u_prev, v_uv + vec2(0.0,-px.y)).r;
    n += texture2D(u_prev, v_uv + vec2( px.x,-px.y)).r + texture2D(u_prev, v_uv + vec2(-px.x,0.0)).r;
    n += texture2D(u_prev, v_uv + vec2( px.x, 0.0)).r  + texture2D(u_prev, v_uv + vec2(-px.x,px.y)).r;
    n += texture2D(u_prev, v_uv + vec2( 0.0,  px.y)).r + texture2D(u_prev, v_uv + vec2( px.x,px.y)).r;

    float self_alive = texture2D(u_prev, v_uv).r;

    // During wipe window: zero neighbours and self so CA rules produce all-dead,
    // then seed-gate below re-fires density noise fresh.
    n          *= (1.0 - in_wipe);
    self_alive *= (1.0 - in_wipe);

    float n_int = floor(n + 0.5);
    float born    = step(2.5, n_int) * step(n_int, 3.5);
    float survive = step(1.5, n_int) * step(n_int, 3.5);
    float next_alive = max(step(0.5, self_alive) * survive,
                           (1.0 - step(0.5, self_alive)) * born);

    float life = max(next_alive, self_alive * u_param3); // trail

    // Seed phase (first second) OR wipe window: re-seed from density.
    float t_seed = step(u_time, 1.0);
    float seed_gate = max(t_seed, in_wipe);
    life = max(life, step(1.0 - u_param0, hash21(gp + floor(u_time * 60.0))) * seed_gate);

    // Beat/trigger heavy reseed: keeps board lively on every beat.
    // Suppressed during wipe so noise doesn't immediately cancel the clear.
    float beat_gate = max(step(0.4, u_beat), step(0.4, u_trigger));
    life = max(life, step(1.0 - (0.05 + 0.10 * u_param1),
                          hash21(gp + floor(u_time * 4.0) * 7.3)) * beat_gate * (1.0 - t_seed) * (1.0 - in_wipe));

    // Activity floor: ~2% noise every 2 s — prevents total extinction.
    float floor_gate = step(mod(u_time, 2.0), 0.05) * (1.0 - t_seed) * (1.0 - in_wipe);
    life = max(life, step(0.98, hash21(gp + floor(u_time * 0.5) * 3.1)) * floor_gate);

    // Bass kick: stamp a 5-cell glider cluster at a hashed anchor.
    float bkt = floor(u_time * 2.0);
    vec2 anchor = floor(vec2(hash21(vec2(bkt, 1.0)) * u_resolution.x,
                             hash21(vec2(bkt, 2.0)) * u_resolution.y));
    vec2 off = gp - anchor;
    float is_glider =
        step(abs(off.x), 0.5) * step(abs(off.y), 0.5) +
        step(abs(off.x - 1.0), 0.5) * step(abs(off.y), 0.5) +
        step(abs(off.x - 2.0), 0.5) * step(abs(off.y), 0.5) +
        step(abs(off.x - 2.0), 0.5) * step(abs(off.y - 1.0), 0.5) +
        step(abs(off.x - 1.0), 0.5) * step(abs(off.y - 2.0), 0.5);
    life = max(life, clamp(is_glider, 0.0, 1.0) * step(0.5, u_audio.x) * (1.0 - t_seed) * (1.0 - in_wipe));

    // Colour: red = raw life; green/blue = hue tint.
    vec3 tint = 0.5 + 0.5 * cos(6.2831 * (u_param2 + vec3(0.0, 0.33, 0.66)));
    gl_FragColor = vec4(life, tint.g * life, tint.b * life, 1.0);
}
