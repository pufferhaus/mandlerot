// Fire — Doom-style heat-bleed cellular automaton.
//
// Uses u_prev as the heat buffer (red channel = heat value 0..1).
// Heat source is at screen bottom (v_uv.y = 1.0). Each frame, pixels
// average heat from "below" (higher v_uv.y = closer to bottom = source),
// then decay. Result rises toward v_uv.y = 0 (screen top).
//
// Color ramp: black → red → orange → yellow → white
//
// u_param0  decay   (0..1, default 0.3) — how fast heat dissipates

void main() {
    vec2 px = 1.0 / u_resolution;
    float heat;

    if (v_uv.y > 0.96) {
        // Heat source at screen bottom; bass boosts intensity
        heat = 0.85 + u_audio.x * 0.15;
    } else {
        float px_w = px.x;
        float px_h = px.y;
        // Sample below (higher y = closer to bottom source) and neighbors
        float s0 = texture2D(u_prev, v_uv + vec2(-px_w, px_h * 2.0)).r;
        float s1 = texture2D(u_prev, v_uv + vec2( 0.0,  px_h * 2.0)).r;
        float s2 = texture2D(u_prev, v_uv + vec2( px_w, px_h * 2.0)).r;
        float s3 = texture2D(u_prev, v_uv + vec2( 0.0,  px_h)).r;
        heat = (s0 + s1 * 1.5 + s2 + s3 * 1.5) / 5.5;
        float decay = u_param0 * 0.06 + 0.005;
        heat = max(0.0, heat - decay);
    }

    float r = clamp(heat * 3.0,       0.0, 1.0);
    float g = clamp(heat * 3.0 - 1.0, 0.0, 1.0);
    float b = clamp(heat * 3.0 - 2.0, 0.0, 1.0);
    gl_FragColor = vec4(r, g, b, 1.0);
}
