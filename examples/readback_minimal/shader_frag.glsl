#version 330 core
out vec4 fragColor;
in vec2 v_uv;
uniform float u_time;

void main() {
    // simple animated gradient
    vec2 uv = v_uv;
    float r = 0.5 + 0.5 * sin(u_time + uv.x * 6.2831);
    float g = 0.5 + 0.5 * sin(u_time * 0.7 + uv.y * 6.2831);
    float b = 0.5 + 0.5 * sin(u_time * 1.3);
    fragColor = vec4(r, g, b, 1.0);
}
