// 03_mandala.frag — Radially symmetric SDF mandala
// N-fold symmetry with rotating geometry. CC7=0 → key mode.
//
// CC1=speed  CC2=symmetry (3-12 fold)  CC3=ring count  CC4=hue
// CC5=inner radius  CC6=petal size  CC7=bg (0=key/black)  CC8=spin speed

uniform float u_p1;
uniform float u_p2;
uniform float u_p3;
uniform float u_p4;
uniform float u_p5;
uniform float u_p6;
uniform float u_p7;
uniform float u_p8;

float sdBox(vec2 p, vec2 b) {
    vec2 d = abs(p) - b;
    return length(max(d, 0.0)) + min(max(d.x, d.y), 0.0);
}
float sdCircle(vec2 p, float r) { return length(p) - r; }
float smin_m(float a, float b, float k) {
    float h = clamp(0.5 + 0.5*(b-a)/k, 0.0, 1.0);
    return mix(b, a, h) - k*h*(1.0-h);
}

void main() {
    vec2  uv = (v_uv*2.0 - 1.0);
    uv.x    *= uResolution.x / uResolution.y;
    float px = 2.0 / min(uResolution.x, uResolution.y);
    float t  = uTime * (0.1 + u_p1 * 0.5);
    float spin = t * (0.2 + u_p8 * 0.8);

    // Radial symmetry fold
    float sym = 3.0 + floor(u_p2 * 9.0);  // 3–12
    float r   = length(uv);
    float a   = atan(uv.y, uv.x) + spin;
    float seg = 6.28318 / sym;
    a = mod(a, seg) - seg * 0.5;
    vec2  uvs = vec2(cos(a), sin(a)) * r;  // folded UV

    // Parameters
    float ir   = 0.10 + u_p5 * 0.30;
    float pw   = 0.015 + u_p6 * 0.05;
    int   nrings = 1 + int(u_p3 * 3.0);   // 1–4 rings

    float d = 1e9;

    // Concentric rings at multiples of ir
    for (int i = 0; i < 4; i++) {
        if (i >= nrings) break;
        float ri = ir * (1.0 + float(i) * 0.8);
        d = smin_m(d, abs(r - ri) - pw, 0.01);
    }

    // Radial spokes (boxes in folded space)
    float spoke = sdBox(uvs, vec2(pw * 0.6, ir * 1.5));
    d = smin_m(d, spoke, 0.015);

    // Centre circle
    d = smin_m(d, sdCircle(uv, ir * 0.3), 0.02);

    float fill = 1.0 - smoothstep(-px, px, d);
    float glow = exp(-max(d, 0.0) * 16.0) * 0.4;
    float rim  = 1.0 - smoothstep(0.0, px*3.0, abs(d) - px);

    // Hue shifts radially
    vec3 fg = 0.55 + 0.45*cos(u_p4*6.28318 + r*4.0 - t + vec3(0.0, 2.1, 4.2));
    vec3 bg = mix(vec3(0.0), 0.07*cos(u_p4*6.28318 + 3.14 + vec3(0,2.1,4.2)) + 0.05, u_p7);
    bg = max(bg, 0.0);

    vec3 col = bg + fg*(glow + fill) + vec3(1.0)*rim*fill*0.3;
    col = col / (1.0 + col);
    col = pow(clamp(col, 0.0, 1.0), vec3(0.4545));
    fragColor = vec4(col, 1.0);
}
