#version 330 core
in vec2 v_uv;
out vec4 o;
uniform sampler2D iChannel0;

void main() {
    vec2 uv = vec2(1.0 - v_uv.x, 1.0 - v_uv.y);
    o = texture(iChannel0, uv);
}

