// Self-playing Pong. The ball trajectory and paddle positions are computed
// closed-form so no persistent state is needed — every fragment recovers
// the ball's current x,y from u_time + a few hashes for "match resets."
//
// The match resets every `match_secs`; within a match the ball bounces off
// the top/bottom walls and the paddles, which track the ball with imperfect
// lag (proportional follow with capped speed). Audio modulates ball speed.
//
// u_param0 ball_speed    0.5..3    ball pixels-per-second multiplier (1.0)
// u_param1 paddle_height 0.05..0.4 fraction of screen height (0.18)
// u_param2 paddle_lag    0..1      paddle imperfection (0.4)
// u_param3 trail         0..1      ball motion trail length (0.4)
// u_param4 scanlines     0..1      CRT scanline depth (0.5)
// u_param5 fg_hue        0..1      paddle / ball color
// u_param6 bg_hue        0..1      court background hue
// u_param7 match_secs    5..60     seconds per match before reset (18)

float h(vec2 p){ return fract(sin(dot(p, vec2(12.9898,78.233))) * 43758.5453); }

// Compute ball position at time t given speed and a per-match seed.
vec2 ball_at(float t, float speed, float seed) {
    // Initial pos + velocity from seed
    vec2 p = vec2(0.1 + 0.8 * h(vec2(seed, 0.0)), 0.4 + 0.2 * h(vec2(seed, 1.0)));
    float ang = (h(vec2(seed, 2.0)) - 0.5) * 1.0; // -0.5..0.5 rad bias
    vec2 v = vec2(cos(ang), sin(ang)) * speed;
    // unfold by walking: reflect off top/bottom only (paddles too fast to
    // miss). x bounces between 0.06..0.94 (paddle inner faces).
    p += v * t;
    // reflect y
    float y = p.y;
    float fy = mod(y, 2.0);
    if (fy > 1.0) fy = 2.0 - fy;
    // reflect x off paddle planes (0.06 and 0.94)
    float xspan = 0.88;
    float xrel = p.x - 0.06;
    float xmod = mod(xrel, 2.0 * xspan);
    if (xmod > xspan) xmod = 2.0 * xspan - xmod;
    return vec2(0.06 + xmod, fy);
}

void main() {
    vec2 uv = v_uv;

    float match_t = max(u_param7, 3.0);
    float match_idx = floor(u_time / match_t);
    float t_in_match = mod(u_time, match_t);
    float seed = match_idx;

    float speed = u_param0 * (0.18 + u_audio.x * 0.08);
    vec2 ball = ball_at(t_in_match, speed, seed);

    // paddle target = ball.y; paddle position lags via simple low-pass-equivalent
    // closed form: weighted blend of current ball.y and a slightly-past ball.y
    vec2 ball_past = ball_at(t_in_match - 0.4 * u_param2, speed, seed);
    float left_y = mix(ball.y, ball_past.y, u_param2);
    float right_y = mix(ball.y, ball_at(t_in_match - 0.25 * u_param2, speed, seed).y, u_param2);

    // colors
    vec3 fg = 0.5 + 0.5 * cos(6.2831 * (u_param5 + vec3(0.0, 0.33, 0.66)));
    vec3 bg = 0.5 + 0.5 * cos(6.2831 * (u_param6 + vec3(0.0, 0.33, 0.66))) * 0.2;
    vec3 col = bg;

    // court: dashed center line
    float center = step(abs(uv.x - 0.5), 0.005) * step(0.5, fract(uv.y * 16.0));
    col = mix(col, fg * 0.6, center);

    // top/bottom walls
    float walls = step(uv.y, 0.02) + step(0.98, uv.y);
    col = mix(col, fg, walls);

    // paddles
    float ph = u_param1;
    float pw = 0.015;
    float left = step(abs(uv.x - 0.05), pw) * step(abs(uv.y - left_y), ph * 0.5);
    float right = step(abs(uv.x - 0.95), pw) * step(abs(uv.y - right_y), ph * 0.5);
    col = mix(col, fg, max(left, right));

    // ball + trail
    float dist = length(uv - ball);
    float ball_mask = smoothstep(0.012, 0.0, dist);
    col = mix(col, fg, ball_mask);
    // trail: look backward along the velocity (approx by sampling earlier ball positions)
    float trail = 0.0;
    for (int i = 1; i <= 6; i++) {
        float dt = float(i) * 0.06;
        vec2 b = ball_at(t_in_match - dt, speed, seed);
        float d = length(uv - b);
        trail += smoothstep(0.012, 0.0, d) * (1.0 - float(i) / 6.0) * u_param3;
    }
    col += fg * trail * 0.4;

    // scanlines
    col *= mix(1.0, 0.5 + 0.5 * sin(uv.y * u_resolution.y * 3.1415), u_param4);

    gl_FragColor = vec4(clamp(col, 0.0, 1.5), 1.0);
}
