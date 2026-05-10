// Single-line composite overlay.
// Renders a 6x8 pixel ASCII glyph atlas as a small horizontal strip.
// Atlas is supplied as a texture; characters are addressed by ASCII code
// (32-126). The text content is encoded into a 1xN luminance texture
// where each texel's red channel is the ASCII code / 255.
#version 100
precision mediump float;
uniform sampler2D u_overlay_tex;  // RGBA8 texture: pre-rasterized strip
uniform vec2 u_resolution;         // panel resolution
uniform vec2 u_overlay_size;       // overlay strip size in pixels
uniform vec2 u_overlay_origin;     // top-left in pixel coords
varying vec2 v_uv;

void main() {
    vec2 pixel = v_uv * u_resolution;
    vec2 in_overlay = pixel - u_overlay_origin;
    if (in_overlay.x < 0.0 || in_overlay.x >= u_overlay_size.x ||
        in_overlay.y < 0.0 || in_overlay.y >= u_overlay_size.y) {
        discard;
    }
    vec2 uv = in_overlay / u_overlay_size;
    vec4 c = texture2D(u_overlay_tex, uv);
    if (c.a < 0.05) discard;
    gl_FragColor = c;
}
