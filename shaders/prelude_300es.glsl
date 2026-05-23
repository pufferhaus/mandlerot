#version 300 es
precision mediump float;

uniform float u_time;
uniform vec2  u_resolution;
uniform vec4  u_audio;
uniform float u_audio_mid;
uniform float u_beat;
uniform float u_bpm;
uniform float u_trigger;
uniform sampler2D u_prev;
uniform float u_param0;
uniform float u_param1;
uniform float u_param2;
uniform float u_param3;
uniform float u_param4;
uniform float u_param5;
uniform float u_param6;
uniform float u_param7;
uniform float u_param8;
uniform sampler2D u_audio_history; // RGBA8 1x320: each row = one frame's bands (R=bass, G=lomid, B=himid, A=treble); v=0 oldest, v=1 newest.
uniform sampler2D u_video;          // live external feed (1280x720 max RGBA8); empty = 1x1 black
uniform vec2      u_video_uv_scale; // multiply v_uv by this to sample only the populated rect

in vec2 v_uv;
out vec4 fragColor;
