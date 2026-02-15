#version 330 core
in vec2 vUv;
out vec4 oColor;

uniform sampler2D u_tex0;

void main() {
  oColor = texture(u_tex0, vUv);
}
