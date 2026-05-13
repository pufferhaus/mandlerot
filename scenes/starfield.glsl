// u_param0  density       (10..200) — star count (floored, loop max 200)
// u_param1  speed         (0..3)   — radial speed [bass]
// u_param2  streak_length (0..0.05) — star streak size
// u_param3  hue           (0..1)   — base hue
// u_param4  hue_variance  (0..0.5) — per-star hue spread
//
// Star tunnel: hash seeds radial star positions, project outward over time.
// BPM not directly used (speed param controls rate); bass drives speed boost.
float hash(float n) { return fract(sin(n * 127.1) * 43758.5453); }

void main() {
    // Use normalized space [-0.5..0.5] so star radii are in consistent units
    vec2 uv = v_uv - 0.5;

    int count = int(u_param0);
    float speed = u_param1; // toml routes bass to speed
    float streak = u_param2;
    float hue = u_param3 + u_time * 0.02; // slow continuous hue drift
    float hue_var = u_param4;

    vec3 col = vec3(0.0);

    for (int i = 0; i < 80; i++) {
        if (i >= count) break;
        float fi = float(i);

        // Seed angle and initial depth for each star
        float angle = hash(fi * 1.3) * 6.2831;
        float depth = fract(hash(fi * 2.7) + speed * u_time * 0.1);
        float r = depth * 0.7; // radial distance from center (0..0.7)

        vec2 star_pos = vec2(cos(angle), sin(angle)) * r;

        // Streak: draw a short segment from r-streak toward center
        vec2 d = uv - star_pos;
        vec2 streak_dir = normalize(star_pos + vec2(0.0001));
        float along = dot(d, streak_dir);
        float perp  = abs(d.x * streak_dir.y - d.y * streak_dir.x);

        float in_streak = step(0.0, along) * step(along, streak) * step(perp, 0.003);
        float brightness = depth * depth; // brighter at edges
        float star_hue = hue + hue_var * (hash(fi * 5.1) - 0.5) * 2.0;
        vec3 star_col = 0.5 + 0.5 * cos(6.2831 * (star_hue + vec3(0.0, 0.33, 0.66)));
        col += star_col * in_streak * brightness * 2.0;
    }

    col = clamp(col, 0.0, 1.0);
    gl_FragColor = vec4(col, 1.0);
}
