// Pseudo-flocking boids. No real simulation (we have no persistent state
// in a single-pass fragment shader). Instead we follow ~12 attractor
// points whose positions are smoothed by time, and render each "boid" as
// a small triangle pointing along its velocity vector.
//
// The illusion of flocking comes from the attractors clustering and
// drifting, plus the boids being drawn very small so the viewer reads them
// as a swarm rather than as individual fish.
//
// u_param0 boid_count    8..64    number of boids (32)
// u_param1 size          0.005..0.04 boid triangle size (0.012)
// u_param2 flock_radius  0.05..0.4 clustering tightness
// u_param3 drift_speed   0..2     overall motion rate
// u_param4 hue           0..1     boid color
// u_param5 bg_hue        0..1     backdrop hue
// u_param6 audio_swarm   0..1     bass tightens the flock, treble scatters
// u_param7 trail         0..1     short motion trails

float h(vec2 p){ return fract(sin(dot(p, vec2(12.9898,78.233))) * 43758.5453); }
float h1(float x){ return fract(sin(x * 12.9898) * 43758.5453); }

// Triangle distance: signed distance to an isoceles triangle of width w,
// height h pointing +y, then we rotate the sample so triangle aims at vel.
float tri(vec2 p, float w, float ht) {
    p.x = abs(p.x);
    float a = p.x * ht / w + p.y - ht * 0.5;
    float b = -p.y - ht * 0.5;
    return max(a, b);
}

void main() {
    vec2 uv = (v_uv - 0.5) * vec2(u_resolution.x / u_resolution.y, 1.0);

    vec3 bg = 0.5 + 0.5 * cos(6.2831 * (u_param5 + vec3(0.0, 0.33, 0.66))) * 0.2;
    vec3 fg = 0.5 + 0.5 * cos(6.2831 * (u_param4 + vec3(0.0, 0.33, 0.66)));
    vec3 col = bg;

    // Flock attractor: a slowly moving center in screen space
    vec2 flock = vec2(
        sin(u_time * 0.31) * 0.5,
        cos(u_time * 0.27) * 0.35
    );
    float radius = u_param2 * (1.0 - u_audio.x * u_param6 + u_audio.w * u_param6 * 0.6);

    float n = max(u_param0, 4.0);
    for (int i = 0; i < 64; i++) {
        if (float(i) >= n) break;
        float fi = float(i);
        // each boid orbits the flock attractor with its own phase
        float phase = h1(fi * 0.197) * 6.2831;
        float orbit_speed = (0.4 + h1(fi * 0.31) * 0.8) * u_param3;
        float t = u_time * orbit_speed + phase;
        vec2 offset = radius * vec2(cos(t + fi), sin(t * 1.13 + fi * 1.7));
        // long-period drift adds individuality
        offset += 0.05 * vec2(sin(u_time * 0.5 + fi * 0.7), cos(u_time * 0.43 + fi));
        vec2 pos = flock + offset;
        // velocity = derivative of pos w.r.t. t (approximation)
        vec2 vel = vec2(-sin(t + fi), 1.13 * cos(t * 1.13 + fi * 1.7));
        float ang = atan(vel.y, vel.x) - 1.5707;
        mat2 R = mat2(cos(ang), -sin(ang), sin(ang), cos(ang));
        vec2 local = R * (uv - pos);
        float d = tri(local, u_param1, u_param1 * 1.6);
        float body = smoothstep(0.002, 0.0, d);
        col = mix(col, fg, body);

        // optional trail: small earlier-position dot
        if (u_param7 > 0.0) {
            float dt = 0.08;
            vec2 v_prev = vec2(-sin(t + fi - dt), 1.13 * cos((t - dt) * 1.13 + fi * 1.7));
            vec2 prev_pos = pos - vel * dt * 0.1;
            float td = length(uv - prev_pos);
            col += fg * smoothstep(0.008, 0.0, td) * u_param7 * 0.4;
        }
    }

    gl_FragColor = vec4(col, 1.0);
}
