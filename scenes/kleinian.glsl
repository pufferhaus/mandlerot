// u_param0  orbit_speed  (0..2, 0.25)
// u_param1  cam_distance (2..6, 3.2)
// u_param2  hue          (0..1, 0.18)
// u_param3  detail       (16..96, 56)
// u_param4  iterations   (4..12, 8)
// u_param5  csize_x      (0.7..1.3, 0.97) — fold cell x  [bass +]
// u_param6  csize_y      (0.7..1.3, 1.0)
// u_param7  csize_z      (0.7..1.5, 1.2)  — fold cell z  [himid +]
//
// Pseudo-Kleinian: inversive fold against a box (CSize), repeated. Generates
// strange organic / architectural forms. Animating CSize (audio-routed) shifts
// the topology dramatically.

float DE(vec3 p, vec3 CSize, int iters){
    float DEfactor = 1.0;
    for (int i = 0; i < 12; i++){
        if (i >= iters) break;
        p = 2.0 * clamp(p, -CSize, CSize) - p;
        float r2 = dot(p, p);
        float k = max(0.7 / max(r2, 1e-4), 1.0);
        p *= k;
        DEfactor *= k;
    }
    float rxy = length(p.xy);
    return 0.5 * (rxy - 1.0) / max(DEfactor, 1e-4);
}

void main(){
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;
    float orbit = u_param0;
    float dist  = u_param1;
    float hue   = u_param2;
    int   steps = int(u_param3);
    int   iters = int(u_param4);
    vec3  CS    = vec3(u_param5, u_param6, u_param7);

    float aspect = u_resolution.x / u_resolution.y;
    float angle = u_time * orbit * bpm / 60.0 * 0.1;

    vec3 ro = vec3(cos(angle) * dist, sin(u_time * 0.19) * 1.2, sin(angle) * dist);
    vec3 fwd = normalize(-ro);
    vec3 right = normalize(cross(fwd, vec3(0.0, 1.0, 0.0)));
    vec3 up = cross(right, fwd);

    vec2 ndc = (v_uv - 0.5) * vec2(aspect, 1.0);
    vec3 rd = normalize(fwd + ndc.x * right + ndc.y * up);

    float t = 0.0;
    float hit = 0.0;
    float escape = 0.0;
    for (int j = 0; j < 96; j++){
        if (j >= steps) break;
        vec3 p = ro + rd * t;
        float d = DE(p, CS, iters);
        if (d < 0.001){ hit = 1.0; escape = float(j) / float(steps); break; }
        if (t > 10.0) break;
        t += max(d, 0.001);
    }

    if (hit < 0.5){ gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0); return; }

    float audio_drift = u_audio.x * 0.12;
    float bpm_drift = u_time * bpm / (60.0 * 32.0);
    float col_t = escape + hue + audio_drift + bpm_drift + u_beat * 0.05;
    vec3 col = 0.5 + 0.5 * cos(6.2831 * (col_t + vec3(0.0, 0.33, 0.66)));
    float shade = 1.0 - clamp(t / 10.0, 0.0, 0.7);
    col *= shade;

    gl_FragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
