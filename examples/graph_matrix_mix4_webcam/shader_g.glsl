#version 330 core
uniform vec2 uResolution;
uniform float uTime;
out vec4 FragColor;

void main() {
    vec2 uv = gl_FragCoord.xy / uResolution.xy;
    float v = 0.5 + 0.5 * sin(uv.y * 12.0 - uTime);
    FragColor = vec4(0.0, v, 0.0, 1.0);
}

