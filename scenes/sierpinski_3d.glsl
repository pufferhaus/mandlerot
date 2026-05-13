// u_param0  orbit_speed  (0..2, 0.25)
// u_param1  cam_distance (1.5..6, 3.0)
// u_param2  hue          (0..1, 0.42)
// u_param3  detail       (16..96, 48)
// u_param4  iterations   (5..18, 12)
// u_param5  scale        (1.6..2.4, 2.0) — IFS scale [bass +]
// u_param6  offset_x     (0..1.5, 1.0)   — fold offset x
// u_param7  offset_y     (0..1.5, 1.0)   — fold offset y [himid +]
//
// Sierpinski 3D: kaleidoscopic IFS tetrahedron. Folds (negate-swap) along
// 3 diagonal planes, then scale+translate. Generates crystalline triangular
// fractal — sharp planar facets.

float DE(vec3 p, int iters, float scale, vec3 offset){
    int n = 0;
    for (n = 0; n < 12; n++){
        if (n >= iters) break;
        if (p.x + p.y < 0.0) p.xy = -p.yx;
        if (p.x + p.z < 0.0) p.xz = -p.zx;
        if (p.y + p.z < 0.0) p.zy = -p.yz;
        p = p * scale - offset * (scale - 1.0);
    }
    return length(p) * pow(scale, -float(iters));
}

void main(){
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;
    float orbit = u_param0;
    float dist  = u_param1;
    float hue   = u_param2;
    int   steps = int(u_param3);
    int   iters = int(u_param4);
    float scale = u_param5;
    vec3  off   = vec3(u_param6, u_param7, 1.0);

    float aspect = u_resolution.x / u_resolution.y;
    float angle = u_time * orbit * bpm / 60.0 * 0.1;

    vec3 ro = vec3(cos(angle) * dist, sin(u_time * 0.18) * 1.2, sin(angle) * dist);
    vec3 fwd = normalize(-ro);
    vec3 right = normalize(cross(fwd, vec3(0.0, 1.0, 0.0)));
    vec3 up = cross(right, fwd);

    vec2 ndc = (v_uv - 0.5) * vec2(aspect, 1.0);
    vec3 rd = normalize(fwd + ndc.x * right + ndc.y * up);

    float t = 0.0;
    float hit = 0.0;
    float escape = 0.0;
    for (int j = 0; j < 32; j++){
        if (j >= steps) break;
        vec3 p = ro + rd * t;
        float d = DE(p, iters, scale, off);
        if (d < 0.003){ hit = 1.0; escape = float(j) / float(steps); break; }
        if (t > 8.0) break;
        t += max(d, 0.001);
    }

    if (hit < 0.5){ gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0); return; }

    float audio_drift = u_audio.x * 0.10;
    float bpm_drift = u_time * bpm / (60.0 * 32.0);
    float col_t = escape + hue + audio_drift + bpm_drift + u_beat * 0.05;
    vec3 col = 0.5 + 0.5 * cos(6.2831 * (col_t + vec3(0.0, 0.33, 0.66)));
    float shade = 1.0 - clamp(t / 8.0, 0.0, 0.7);
    col *= shade;

    gl_FragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
