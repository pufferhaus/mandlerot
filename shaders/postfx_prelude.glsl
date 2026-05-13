#version 100
precision mediump float;

// Post-FX prelude: passes see the previous pass's RGBA texture as `u_input`,
// the last frame's final chain output as `u_prev` (for trails / persistence),
// plus time and the 8 param slots. Audio routing on a post-fx param still
// works because the host pre-resolves slot values via
// `ParamMap::effective_slot_values` before uploading.
uniform sampler2D u_input;
uniform sampler2D u_prev;
uniform vec2  u_resolution;
uniform float u_time;
uniform float u_audio_mid;
uniform float u_param0;
uniform float u_param1;
uniform float u_param2;
uniform float u_param3;
uniform float u_param4;
uniform float u_param5;
uniform float u_param6;
uniform float u_param7;

varying vec2 v_uv;
