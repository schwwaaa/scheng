#version 330 core
uniform vec2 uResolution;
uniform float uTime;
out vec4 FragColor;

void main() {
    vec2 uv = gl_FragCoord.xy / uResolution.xy;
    vec2 p = uv - 0.5;
    float r = length(p);

    float bands = 0.5 + 0.5 * cos(20.0 * r - uTime * 1.3);
    float swirl = 0.5 + 0.5 * sin(8.0 * (p.x + p.y) + uTime * 0.6);

    vec3 color = vec3(bands * 0.8, swirl * 0.6, (1.0 - r) * 0.7);
    FragColor = vec4(color, 1.0);
}

