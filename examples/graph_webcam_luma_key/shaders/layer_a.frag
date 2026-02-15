#version 330 core
uniform vec2 uResolution;
uniform float uTime;
out vec4 FragColor;

void main() {
    vec2 uv = gl_FragCoord.xy / uResolution.xy;
    float v = 0.5 + 0.5 * sin(uv.x * 10.0 + uTime * 0.7);
    FragColor = vec4(v, 0.2 * v, 0.1 + 0.3 * v, 1.0);
}

