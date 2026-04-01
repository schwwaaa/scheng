// 04_noise.frag — fractal Brownian motion (fBm)
// Layered value noise — the foundation of most organic textures.
//
// u_p1 = animation speed
// u_p2 = scale
// u_p3 = octaves (1–6)
// u_p4 = turbulence amount
// u_p5 = color mode

float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

float noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(hash(i),          hash(i + vec2(1,0)), u.x),
        mix(hash(i + vec2(0,1)), hash(i + vec2(1,1)), u.x),
    u.y);
}

float fbm(vec2 p, int oct) {
    float v = 0.0, a = 0.5;
    for (int i = 0; i < 6; i++) {
        if (i >= oct) break;
        v += a * noise(p);
        p  = p * 2.1 + vec2(1.7, 9.2);
        a *= 0.5;
    }
    return v;
}

uniform float u_p1;
uniform float u_p2;
uniform float u_p3;
uniform float u_p4;
uniform float u_p5;
uniform float u_p6;
uniform float u_p7;
uniform float u_p8;
void main() {
    float t    = uTime * (0.1 + u_p1 * 0.4);
    float sc   = 2.0 + u_p2 * 6.0;
    int   oct  = 1 + int(u_p3 * 5.0);
    vec2  uv   = v_uv * sc + vec2(t, t * 0.7);

    // Turbulence — domain warp
    float warp = u_p4 * 2.0;
    vec2  q    = vec2(fbm(uv, oct), fbm(uv + vec2(5.2, 1.3), oct));
    float n    = fbm(uv + warp * q, oct);

    // Color
    vec3 col;
    float mode = u_p5;
    if (mode < 0.33) {
        col = mix(vec3(0.1, 0.2, 0.5), vec3(0.9, 0.95, 1.0), n);
    } else if (mode < 0.66) {
        col = mix(vec3(0.05, 0.3, 0.1), vec3(0.8, 1.0, 0.6), n * n);
    } else {
        col = mix(vec3(0.3, 0.05, 0.1), vec3(1.0, 0.8, 0.4), pow(n, 0.8));
    }

    fragColor = vec4(col, 1.0);
}
