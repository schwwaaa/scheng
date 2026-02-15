#version 330 core
uniform vec2 uResolution;
uniform float uTime;
out vec4 FragColor;

void main() {
    vec2 uv = gl_FragCoord.xy / uResolution.xy;
    float v = 0.5 + 0.5 * cos((uv.x + uv.y) * 8.0 + uTime);
    FragColor = vec4(0.0, 0.0, v, 1.0);
}

