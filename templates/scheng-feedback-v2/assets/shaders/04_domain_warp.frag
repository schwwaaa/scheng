// ═══════════════════════════════════════════════════════════════════════════
// LESSON 04 — Domain Warping & Fractal Noise
// Bending space to create organic flowing patterns.
// ═══════════════════════════════════════════════════════════════════════════
//
// DOMAIN WARPING: instead of moving shapes, warp the coordinate space
// before evaluating any pattern:
//   warped_uv = uv + warp_strength * vec2(sin(uv.y * freq), cos(uv.x * freq));
//
// FRACTAL BROWNIAN MOTION (fbm): stack octaves of noise at increasing
// frequency and decreasing amplitude. Each layer adds finer detail.
//   fbm = noise(p)*0.5 + noise(p*2)*0.25 + noise(p*4)*0.125 + ...
//
// RECURSIVE WARP: warp using fbm, then feed the result back as the warp
// input — creates extremely organic, cloud-like turbulence.
//
// ─────────────────────────────────────────────────────────────────────────
// CC1 = speed         CC2 = warp strength   CC3 = warp frequency
// CC4 = recursion (1/2/3 passes)            CC5 = colour palette
// CC6 = brightness    CC7 = scale           CC8 = warp direction

uniform float u_p1;
uniform float u_p2;
uniform float u_p3;
uniform float u_p4;
uniform float u_p5;
uniform float u_p6;
uniform float u_p7;
uniform float u_p8;

float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);  // smooth interpolation
    return mix(
        mix(hash(i),           hash(i+vec2(1,0)), u.x),
        mix(hash(i+vec2(0,1)), hash(i+vec2(1,1)), u.x),
        u.y
    );
}

float fbm(vec2 p) {
    float val = 0.0, amp = 0.5;
    for (int i = 0; i < 5; i++) {
        val += amp * noise(p);
        p   = p * 2.1 + vec2(1.7, 9.2);
        amp *= 0.5;
    }
    return val;
}

void main() {
    float t    = uTime * (0.04 + u_p1 * 0.25);
    float sc   = 1.5 + u_p7 * 4.0;
    vec2  uv   = v_uv * sc;
    float warp = u_p2 * 3.0;
    float freq = 0.8 + u_p3 * 3.0;
    vec2  dir  = vec2(1.0 + u_p8, 1.0 - u_p8 * 0.5);

    vec2 off1 = vec2(t*0.3, t*0.4);
    vec2 off2 = vec2(t*0.2 + 5.2, t*0.3 + 1.3);

    // CC4 controls how many warp passes
    float n;
    if (u_p4 < 0.33) {
        // Pass 1 — plain fbm
        n = fbm(uv * freq + t);
    } else if (u_p4 < 0.66) {
        // Pass 2 — warp by one layer of fbm
        vec2 q = vec2(fbm(uv*freq + off1), fbm(uv*freq + off2));
        n = fbm(uv*freq + warp*q*dir);
    } else {
        // Pass 3 — warp of a warp (most turbulent)
        vec2 q = vec2(fbm(uv*freq + off1), fbm(uv*freq + off2));
        vec2 r = vec2(fbm(uv*freq + warp*q + off1*1.3), fbm(uv*freq + warp*q + off2*1.3));
        n = fbm(uv*freq + warp*r*dir);
    }

    float bri = 0.3 + u_p6 * 0.7;
    vec3 col;
    if      (u_p5 < 0.33) col = mix(vec3(0.05,0.1,0.4),  vec3(0.9,0.95,1.0), n)     * bri;
    else if (u_p5 < 0.66) col = mix(vec3(0.08,0.04,0.02), vec3(1.0,0.65,0.2), pow(n,1.5)) * bri;
    else                   col = mix(vec3(0.04,0.18,0.08), vec3(0.4,1.0,0.45), n*n)  * bri;

    fragColor = vec4(col, 1.0);
}
