#version 330 core
in vec2 v_uv;
out vec4 o;
uniform sampler2D iChannel0;

void main() {
    o = texture(iChannel0, v_uv);
}

