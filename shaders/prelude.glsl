#version 100
precision mediump float;

uniform float u_time;
uniform vec2  u_resolution;
uniform vec4  u_audio;
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

varying vec2 v_uv;
