// Voxel terrain flyover (Comanche-style). For each screen column, march
// outward in 1-step increments and find the closest height that occludes
// the ray; everything above that y is sky. We march at fixed depth steps
// so it's fragment-shader-affordable (~64 steps per pixel).
//
// u_param0 cam_speed     0..3     forward flight speed (1.0)
// u_param1 cam_height    0.5..3   altitude above terrain (1.0)  [bass +]
// u_param2 detail_scale  0.5..3   horizontal noise frequency (1.0)
// u_param3 fog_density   0..1     fade distance (0.5)
// u_param4 hue           0..1     terrain hue (0.12 amber)
// u_param5 sky_hue       0..1     sky hue (0.6 blue)
// u_param6 horizon       0.4..0.8 horizon line (0.55)
// u_param7 brightness    0..2     gain (1.0)

float h(vec2 p){ return fract(sin(dot(p, vec2(12.9898,78.233))) * 43758.5453); }

float vnoise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f*f*(3.0 - 2.0*f);
    float a = h(i);
    float b = h(i + vec2(1.0, 0.0));
    float c = h(i + vec2(0.0, 1.0));
    float d = h(i + vec2(1.0, 1.0));
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

float terrain(vec2 p) {
    float v = 0.0;
    float a = 0.5;
    for (int i = 0; i < 4; i++) {
        v += a * vnoise(p);
        p *= 2.07;
        a *= 0.5;
    }
    return v;
}

void main() {
    vec2 uv = v_uv;
    float horizon = u_param6;

    vec3 sky_col = 0.5 + 0.5 * cos(6.2831 * (u_param5 + vec3(0.0, 0.33, 0.66)));
    sky_col *= mix(0.3, 1.0, uv.y); // brighter toward top

    vec3 ground_col = 0.5 + 0.5 * cos(6.2831 * (u_param4 + vec3(0.0, 0.33, 0.66)));

    if (uv.y > horizon) {
        gl_FragColor = vec4(sky_col * u_param7, 1.0);
        return;
    }

    // ray angle: distance from horizon downward determines depth
    float screen_y = (horizon - uv.y) / horizon;
    // screen_x: -0.5..0.5
    float screen_x = (uv.x - 0.5);
    float aspect = u_resolution.x / u_resolution.y;
    screen_x *= aspect;

    vec3 col = sky_col;
    float t_cam = u_time * u_param0;
    float cam_y = u_param1;

    // March along the ray. For each step, compute world position and ask
    // if the terrain at (x, z) is taller than the ray's y at that depth.
    float prev_h = -1e3;
    float hit = 0.0;
    vec2 hit_p;
    float depth_max = 60.0;
    for (int i = 0; i < 60; i++) {
        float t = float(i) * 1.0 + 1.0; // depth from camera
        if (t > depth_max) break;
        // perspective: ray's y position at depth t (in world units)
        // screen_y maps to a downward angle; ray drops as depth increases
        float ray_y = cam_y - screen_y * t * 0.6;
        // ray's x position scales with depth
        float ray_x = screen_x * t;
        // forward = +z
        float ray_z = t + t_cam;
        float h_here = terrain(vec2(ray_x, ray_z) * 0.08 * u_param2);
        h_here *= 1.6; // taller mountains
        if (h_here > ray_y) {
            hit = 1.0;
            hit_p = vec2(ray_x, ray_z);
            // shade by height + distance fog
            float shade = clamp(h_here, 0.0, 1.5);
            float fog = exp(-t * 0.02 * u_param3);
            col = mix(sky_col, ground_col * (0.35 + 0.65 * shade), fog);
            break;
        }
    }

    // small horizon glow
    float horizon_glow = exp(-abs(uv.y - horizon) * 50.0) * 0.3;
    col += sky_col * horizon_glow;

    col *= u_param7;
    gl_FragColor = vec4(col, 1.0);
}
