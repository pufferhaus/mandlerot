// 3D Maze — homage to the Windows screensaver. First-person walk through
// a hash-generated grid of brick walls. Camera moves forward in z with
// slight x oscillation; a 3-cell-wide corridor at camera's x is always
// open so the camera never enters walls. Brick texture from world-space
// stripes; floor + ceiling planes; fog into distance.
//
// u_param0  walk_speed   (0..2, 0.8)    — forward translation rate [bass +]
// u_param1  wobble       (0..2.5, 1.2)  — camera x oscillation amplitude
// u_param2  wall_hue     (0..1, 0.04)   — brick color
// u_param3  floor_hue    (0..1, 0.32)   — floor/ceiling color [himid +]
// u_param4  density      (0.3..0.7, 0.5)— maze wall density
// u_param5  brick_scale  (0.05..0.3, 0.16) — brick row height [treble +]
// u_param6  fog_distance (4..20, 11.0)  — visibility fade range
// u_param7  bg_tint      (0..0.25, .04) — background brightness

#define MAX_STEPS 56
#define MAX_DIST  22.0
#define SURF_DIST 0.005

float hash2(vec2 v){
    return fract(sin(dot(v, vec2(127.1, 311.7))) * 43758.5453);
}

float sdBox(vec3 p, vec3 b){
    vec3 q = abs(p) - b;
    return length(max(q, 0.0)) + min(max(q.x, max(q.y, q.z)), 0.0);
}

vec3 camPosAt(float t){
    float speed = u_param0 * 1.5;
    float wob   = u_param1;
    return vec3(sin(t * 0.3) * wob, 0.85, t * speed);
}

vec2 mapScene(vec3 p){
    float density = u_param4;
    float cellSize = 2.0;

    // camera's current x cell (used to forge an always-open corridor)
    vec3 cn = camPosAt(u_time);
    float xCamCell = floor(cn.x / cellSize);

    float floorD = p.y;
    float ceilD  = 2.0 - p.y;
    float wallD  = 1e6;

    vec2 cellId = floor(p.xz / cellSize);

    for (int dx = -1; dx <= 1; dx++){
        for (int dz = -1; dz <= 1; dz++){
            vec2 c = cellId + vec2(float(dx), float(dz));
            float h = hash2(c);
            // forge 3-cell-wide z-corridor at camera's x
            if (abs(c.x - xCamCell) <= 1.0) h = 0.0;

            if (h > 1.0 - density){
                vec2 wallCenter = (c + 0.5) * cellSize;
                vec3 q = vec3(p.x - wallCenter.x, p.y - 1.0, p.z - wallCenter.y);
                float bd = sdBox(q, vec3(cellSize*0.5, 1.0, cellSize*0.5));
                wallD = min(wallD, bd);
            }
        }
    }

    // material id: 0=wall, 1=floor, 2=ceiling
    float d = wallD;
    float mid = 0.0;
    if (floorD < d){ d = floorD; mid = 1.0; }
    if (ceilD  < d){ d = ceilD;  mid = 2.0; }
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

// brick pattern on a 2D wall uv (in world units)
vec3 brickPattern(vec2 uv, vec3 baseCol, float scale){
    float rowH  = scale;
    float colW  = scale * 2.2;
    float row   = floor(uv.y / rowH);
    float xOff  = mod(row, 2.0) * 0.5 * colW;
    float fx    = mod((uv.x + xOff), colW) / colW;
    float fy    = mod(uv.y, rowH) / rowH;
    float mx    = step(0.95, fx) + step(fx, 0.05);
    float my    = step(0.92, fy) + step(fy, 0.08);
    float mortar = clamp(mx + my, 0.0, 1.0);
    // per-brick color jitter
    vec2 brickId = vec2(floor((uv.x + xOff) / colW), row);
    float jitter = hash2(brickId) * 0.18 - 0.06;
    vec3 brick = baseCol * (1.0 + jitter);
    return mix(brick, baseCol * 0.18, mortar);
}

void main(){
    vec2 uv = (gl_FragCoord.xy - 0.5*u_resolution) / u_resolution.y;
    float wallHue  = u_param2;
    float floorHue = u_param3;
    float brickS   = u_param5;
    float fogDist  = u_param6;
    float bgTint   = u_param7;

    // camera & look direction (slight head turn following x wobble)
    vec3 ro = camPosAt(u_time);
    vec3 ahead = camPosAt(u_time + 0.6);
    vec3 fwd = normalize(vec3(ahead.x - ro.x, 0.0, ahead.z - ro.z) + vec3(0.0, 0.0, 0.001));

    vec3 right = normalize(cross(vec3(0.0, 1.0, 0.0), fwd));
    vec3 up    = cross(fwd, right);
    vec3 rd    = normalize(fwd + uv.x*right + uv.y*up * 0.9);

    vec2 hit = raymarch(ro, rd);
    float d  = hit.x;
    float mt = hit.y;

    vec3 col = vec3(bgTint);

    if (d < MAX_DIST){
        vec3 p = ro + rd*d;
        vec3 n = calcNormal(p);

        // basic directional light + ambient
        vec3 ldir = normalize(vec3(0.3, 0.9, -0.2));
        float diff = clamp(dot(n, ldir), 0.0, 1.0);
        float ambient = 0.35;

        if (mt < 0.5){
            // wall: brick texture, choose UV from dominant normal axis
            vec3 baseCol = hue2rgb(wallHue) * 0.85;
            vec2 brickUV = abs(n.x) > 0.5 ? vec2(p.z, p.y) : vec2(p.x, p.y);
            col = brickPattern(brickUV, baseCol, brickS);
        } else if (mt < 1.5){
            // floor: checker tint
            vec2 tile = floor(p.xz / 1.0);
            float ch = mod(tile.x + tile.y, 2.0);
            col = hue2rgb(floorHue) * mix(0.35, 0.55, ch);
        } else {
            // ceiling: solid darker
            col = hue2rgb(floorHue + 0.5) * 0.22;
        }

        col *= (ambient + diff * 0.65);

        // distance fog (to bgTint)
        float fog = clamp(d / fogDist, 0.0, 1.0);
        col = mix(col, vec3(bgTint), fog);
    }

    col = pow(col, vec3(0.4545));
    gl_FragColor = vec4(col, 1.0);
}
