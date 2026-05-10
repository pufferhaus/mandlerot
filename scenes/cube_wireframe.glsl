// u_param0  rot_speed  (0..2, 0.4)   — rotation speed [bass 0.6]
// u_param1  line_thick (0.001..0.02, 0.004) — edge line thickness
// u_param2  hue        (0..1, 0.6)   — wireframe color hue
// u_param3  perspective(0..1, 0.5)   — 0=ortho, 1=heavy perspective
// u_param4  size       (0.2..0.8, 0.5) — cube scale in NDC
//
// Rotating 3D cube wireframe: 8 verts, 12 edges, distance-to-segment per pixel.
mat3 cwRotY(float a) {
    float s = sin(a); float c = cos(a);
    return mat3(c, 0.0, s,  0.0, 1.0, 0.0,  -s, 0.0, c);
}
mat3 cwRotX(float a) {
    float s = sin(a); float c = cos(a);
    return mat3(1.0, 0.0, 0.0,  0.0, c, -s,  0.0, s, c);
}

// Return 3D cube corner index 0..7 (+-1 on each axis)
vec3 cwVert(int i) {
    float x = (i == 1 || i == 3 || i == 5 || i == 7) ? 1.0 : -1.0;
    float y = (i == 2 || i == 3 || i == 6 || i == 7) ? 1.0 : -1.0;
    float z = (i >= 4) ? 1.0 : -1.0;
    return vec3(x, y, z);
}

// 2D distance from point p to segment (a, b)
float cwSegDist(vec2 p, vec2 a, vec2 b) {
    vec2 ab = b - a;
    float t = clamp(dot(p - a, ab) / max(dot(ab, ab), 1e-6), 0.0, 1.0);
    return length(p - (a + t * ab));
}

void main() {
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;
    float rot_speed = u_param0 * (1.0 + u_audio.x * 0.6);
    float thickness = u_param1;
    float hue       = u_param2;
    float persp     = u_param3;
    float sz        = u_param4;

    float tY = u_time * rot_speed;
    float tX = u_time * rot_speed * 0.41;
    mat3 rot = cwRotX(tX) * cwRotY(tY);

    // NDC screen coords [-1,1] corrected for aspect
    vec2 aspect = vec2(u_resolution.x / u_resolution.y, 1.0);
    vec2 ndc = (v_uv * 2.0 - 1.0) * aspect;

    // Project each edge: 12 edges as (start, end) vertex index pairs
    // Stored as flat list in if/else chain (GLES 1.00: no arrays of structs)
    float minDist = 1e9;

    for (int e = 0; e < 12; e++) {
        int a = 0; int b = 0;
        if      (e == 0)  { a = 0; b = 1; }
        else if (e == 1)  { a = 2; b = 3; }
        else if (e == 2)  { a = 0; b = 2; }
        else if (e == 3)  { a = 1; b = 3; }
        else if (e == 4)  { a = 4; b = 5; }
        else if (e == 5)  { a = 6; b = 7; }
        else if (e == 6)  { a = 4; b = 6; }
        else if (e == 7)  { a = 5; b = 7; }
        else if (e == 8)  { a = 0; b = 4; }
        else if (e == 9)  { a = 1; b = 5; }
        else if (e == 10) { a = 2; b = 6; }
        else              { a = 3; b = 7; }

        vec3 va = rot * cwVert(a);
        vec3 vb = rot * cwVert(b);

        // Cull fully-behind-camera edges
        if (va.z < -0.1 && vb.z < -0.1) continue;

        // Perspective divide
        float da = 1.0 + va.z * persp * 0.5;
        float db = 1.0 + vb.z * persp * 0.5;
        da = max(da, 0.1); db = max(db, 0.1);
        vec2 pa = va.xy / da * sz;
        vec2 pb = vb.xy / db * sz;

        float d = cwSegDist(ndc, pa, pb);
        if (d < minDist) minDist = d;
    }

    float wire = 1.0 - smoothstep(thickness, thickness * 2.5, minDist);
    wire *= 1.0 + u_beat * 0.5;
    vec3 col = 0.5 + 0.5 * cos(6.2831 * (hue + vec3(0.0, 0.33, 0.66)));
    gl_FragColor = vec4(col * clamp(wire, 0.0, 1.0), 1.0);
}
