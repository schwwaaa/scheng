#version 330 core
uniform vec2 uResolution;
uniform float uTime;
out vec4 FragColor;

void main() {
    vec2 uv = gl_FragCoord.xy / uResolution.xy;
    float v = 0.5 + 0.5 * sin((uv.y * 12.0 - uv.x * 3.0) - uTime * 0.9);
    FragColor = vec4(0.1 + 0.4 * v, 0.3 + 0.5 * v, 0.2, 1.0);
}

