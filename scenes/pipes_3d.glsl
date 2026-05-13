// 3D Pipes — homage to the Windows screensaver. Multiple pipes grow by
// deterministic 90-degree random walks on a 3D lattice. Capsules between
// cells, glossy spheres at joints, periodic cycle reset.
//
// u_param0  spin_speed   (0..2, 0.30)   — camera orbit rate [bass +]
// u_param1  zoom         (4..14, 8.0)   — camera distance
// u_param2  hue_base     (0..1, 0.55)   — base hue, per-pipe offset added [himid +]
// u_param3  growth_rate  (0.2..3.0, 1.0)— segments/sec [bass +]
// u_param4  pipe_radius  (0.05..0.25, 0.14)
// u_param5  reset_period (4..30, 14.0)  — secs per growth cycle
// u_param6  shininess    (0..1, 0.55)   — specular highlight strength [treble +]
// u_param7  bg_tint      (0..0.25, .04) — background brightness

#define MAX_STEPS 14
#define MAX_DIST  18.0
#define SURF_DIST 0.02
#define NPIPES    2
#define NSEG      6

float hash11(float n){ return fract(sin(n * 78.233) * 43758.5453); }

mat2 R(float a){ float s=sin(a), c=cos(a); return mat2(c,-s,s,c); }

vec3 dirIdxToVec(int i){
    if (i == 0) return vec3( 1.0, 0.0, 0.0);
    if (i == 1) return vec3(-1.0, 0.0, 0.0);
    if (i == 2) return vec3( 0.0, 1.0, 0.0);
    if (i == 3) return vec3( 0.0,-1.0, 0.0);
    if (i == 4) return vec3( 0.0, 0.0, 1.0);
    return vec3( 0.0, 0.0,-1.0);
}
int oppIdx(int i){
    if (i == 0) return 1;
    if (i == 1) return 0;
    if (i == 2) return 3;
    if (i == 3) return 2;
    if (i == 4) return 5;
    return 4;
}

float sdCapsule(vec3 p, vec3 a, vec3 b, float r){
    vec3 pa = p - a, ba = b - a;
    float h = clamp(dot(pa, ba)/dot(ba, ba), 0.0, 1.0);
    return length(pa - ba*h) - r;
}

float pipeSDF(vec3 p, float pipeId, float cycle, float lenF, float r){
    float seed = pipeId * 13.37 + cycle * 7.91;
    vec3 node = vec3(
        floor(hash11(seed)       * 6.0) - 3.0,
        floor(hash11(seed + 1.0) * 6.0) - 3.0,
        floor(hash11(seed + 2.0) * 6.0) - 3.0
    );
    int prevDir = int(floor(hash11(seed + 3.0) * 6.0));

    float d = length(p - node) - r * 1.35; // start cap

    for (int k = 1; k < NSEG; k++){
        int newDir = int(floor(hash11(seed + 4.0 + float(k)) * 6.0));
        // avoid reversal
        if (newDir == oppIdx(prevDir)){
            newDir = int(mod(float(newDir + 1), 6.0));
        }
        vec3 next = node + dirIdxToVec(newDir);

        if (float(k) <= lenF){
            float cap = sdCapsule(p, node, next, r);
            d = min(d, cap);
            float jointR = r * 1.35;
            float joint = length(p - next) - jointR;
            d = min(d, joint);
        }

        node = next;
        prevDir = newDir;
    }
    return d;
}

vec2 mapScene(vec3 p){
    float resetPeriod = u_param5;
    float cycle    = floor(u_time / resetPeriod);
    float tInCyc   = u_time - cycle * resetPeriod;
    float lenF     = tInCyc * u_param3;
    float r        = u_param4;

    float d   = 1e6;
    float mid = 0.0;

    for (int i = 0; i < NPIPES; i++){
        float pd = pipeSDF(p, float(i), cycle, lenF, r);
        if (pd < d){
            d = pd;
            mid = float(i) + 0.5;
        }
    }
    return vec2(d, mid);
}

vec2 raymarch(vec3 ro, vec3 rd){
    float t = 0.0;
    float mat = 0.0;
    for (int i = 0; i < MAX_STEPS; i++){
        vec3 p = ro + rd*t;
        vec2 r = mapScene(p);
        mat = r.y;
        if (abs(r.x) < SURF_DIST) break;
        t += r.x;
        if (t > MAX_DIST) break;
    }
    return vec2(t, mat);
}

vec3 calcNormal(vec3 p){
    vec2 k = vec2(1.0, -1.0);
    float h = 0.0015;
    return normalize(
        k.xyy * mapScene(p + k.xyy * h).x +
        k.yyx * mapScene(p + k.yyx * h).x +
        k.yxy * mapScene(p + k.yxy * h).x +
        k.xxx * mapScene(p + k.xxx * h).x
    );
}

vec3 hue2rgb(float h){
    vec3 k = vec3(0.0, 2.0/3.0, 1.0/3.0);
    return clamp(abs(fract(h + k)*6.0 - 3.0) - 1.0, 0.0, 1.0);
}

void main(){
    vec2 uv = (gl_FragCoord.xy - 0.5*u_resolution) / u_resolution.y;
    float spin   = u_param0;
    float zoom   = u_param1;
    float hueB   = u_param2;
    float shiny  = u_param6;
    float bgTint = u_param7;
    float cycle  = floor(u_time / u_param5);

    float a = u_time * spin * 0.4;
    vec3 ro = vec3(0.0, 0.0, zoom);
    ro.xz = R(a) * ro.xz;
    ro.y += sin(u_time * 0.25) * 1.2;

    vec3 fwd   = normalize(-ro);
    vec3 right = normalize(cross(vec3(0.0, 1.0, 0.0), fwd));
    vec3 up    = cross(fwd, right);
    vec3 rd    = normalize(fwd + uv.x*right + uv.y*up);

    vec2 hit = raymarch(ro, rd);
    float d  = hit.x;
    float mt = hit.y;

    float vig = smoothstep(1.4, 0.2, length(uv));
    vec3 col  = vec3(bgTint) * (0.4 + 0.6*vig);

    if (d < MAX_DIST){
        vec3 p = ro + rd*d;
        vec3 n = calcNormal(p);
        vec3 ldir = normalize(vec3(0.5, 0.8, 0.3));
        float diff = clamp(dot(n, ldir), 0.0, 1.0);
        vec3 r = reflect(-ldir, n);
        float spec = pow(clamp(dot(r, -rd), 0.0, 1.0), 18.0);

        float pid = floor(mt);
        float hue = hueB + pid * 0.18 + cycle * 0.07;
        vec3 base = hue2rgb(hue);

        col = base * (0.18 + diff * 0.82) + vec3(1.0) * spec * shiny;
    }

    col = pow(col, vec3(0.4545));
    gl_FragColor = vec4(col, 1.0);
}
