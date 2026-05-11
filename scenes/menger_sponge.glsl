// u_param0  orbit_speed  (0..2, 0.25)
// u_param1  cam_distance (1.8..6, 3.2)  [bass -]
// u_param2  hue          (0..1, 0.12)
// u_param3  detail       (16..96, 48)
// u_param4  iterations   (2..6, 4)      — recursion depth
// u_param5  twist        (-1..1, 0.0)   — y-twist over height [lomid +]
// u_param6  cross_scale  (0.8..1.5, 1.0) — cross arm thickness [bass +]
// u_param7  outer_size   (0.5..1.2, 1.0) — outer box half-size
//
// Menger Sponge: classic recursive cube minus cross arms. Iquilezles
// construction. Twist applies a height-varying y-rotation to the input
// point for dynamic distortion.

float sdBox(vec3 p, vec3 b){
    vec3 q = abs(p) - b;
    return length(max(q, 0.0)) + min(max(q.x, max(q.y, q.z)), 0.0);
}

mat2 rot2(float a){ float s=sin(a), c=cos(a); return mat2(c,-s,s,c); }

float crossDE(vec3 p, float k){
    float da = max(abs(p.x), abs(p.y));
    float db = max(abs(p.y), abs(p.z));
    float dc = max(abs(p.z), abs(p.x));
    return min(da, min(db, dc)) - k;
}

float DE(vec3 p, int iters, float crossScale, float outerSize){
    float d = sdBox(p, vec3(outerSize));
    float s = 1.0;
    for (int m = 0; m < 6; m++){
        if (m >= iters) break;
        vec3 a = mod(p * s, 2.0) - 1.0;
        s *= 3.0;
        vec3 r = abs(1.0 - 3.0 * abs(a));
        float da = max(r.x, r.y);
        float db = max(r.y, r.z);
        float dc = max(r.z, r.x);
        float c = (min(da, min(db, dc)) - crossScale) / s;
        d = max(d, c);
    }
    return d;
}

void main(){
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;
    float orbit = u_param0;
    float dist  = u_param1;
    float hue   = u_param2;
    int   steps = int(u_param3);
    int   iters = int(u_param4);
    float twist = u_param5;
    float crossScale = u_param6;
    float outerSize = u_param7;

    float aspect = u_resolution.x / u_resolution.y;
    float angle = u_time * orbit * bpm / 60.0 * 0.1;

    vec3 ro = vec3(cos(angle) * dist, 0.6 + sin(u_time * 0.15) * 0.4, sin(angle) * dist);
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
        // optional height-varying twist
        p.xz *= rot2(twist * p.y);
        float d = DE(p, iters, crossScale, outerSize);
        if (d < 0.001){ hit = 1.0; escape = float(j) / float(steps); break; }
        if (t > 10.0) break;
        t += max(d, 0.001);
    }

    if (hit < 0.5){ gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0); return; }

    float audio_drift = u_audio.x * 0.10;
    float bpm_drift = u_time * bpm / (60.0 * 32.0);
    float col_t = escape + hue + audio_drift + bpm_drift + u_beat * 0.05;
    vec3 col = 0.5 + 0.5 * cos(6.2831 * (col_t + vec3(0.0, 0.33, 0.66)));
    float shade = 1.0 - clamp(t / 10.0, 0.0, 0.7);
    col *= shade;

    gl_FragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
