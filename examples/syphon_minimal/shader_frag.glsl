#version 330 core
out vec4 FragColor;
void main() {
    // simple gradient; no uniforms needed
    vec2 uv = gl_FragCoord.xy / vec2(1920.0, 1080.0);
    FragColor = vec4(uv.x, uv.y, 0.25 + 0.5*uv.x, 1.0);
}
