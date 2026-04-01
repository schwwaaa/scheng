// ═══════════════════════════════════════════════════════════════════════════
// LESSON 03 — SDF Shape Library
// Building and combining signed distance fields.
// ═══════════════════════════════════════════════════════════════════════════
//
// SDFs can be COMBINED with simple math:
//   min(dA, dB)    → union       (either shape, hard join)
//   max(dA, dB)    → intersection (only where both overlap)
//   max(dA, -dB)   → subtraction  (A minus B)
//   smin(dA,dB,k)  → smooth union (shapes melt together at radius k)
//
// CC1 = shape type (circle/box/rounded box)
// CC2 = shape size     CC3 = smooth blend k
// CC4 = combination mode (union/intersect/subtract/smooth)
// CC5 = rotation       CC6 = glow
// CC7 = colour         CC8 = animation speed

uniform float u_p1;
uniform float u_p2;
uniform float u_p3;
uniform float u_p4;
uniform float u_p5;
uniform float u_p6;
uniform float u_p7;
uniform float u_p8;

float sdCircle(vec2 p, float r) { return length(p) - r; }

float sdBox(vec2 p, vec2 b) {
    vec2 d = abs(p) - b;
    return length(max(d, 0.0)) + min(max(d.x, d.y), 0.0);
}

float sdRoundBox(vec2 p, vec2 b, float r) {
    vec2 d = abs(p) - b + r;
    return length(max(d, 0.0)) + min(max(d.x, d.y), 0.0) - r;
}

// Smooth union — shapes blend into each other over radius k
float smin(float a, float b, float k) {
    float h = clamp(0.5 + 0.5*(b-a)/k, 0.0, 1.0);
    return mix(b, a, h) - k*h*(1.0-h);
}

mat2 rot2(float a) { float c=cos(a),s=sin(a); return mat2(c,-s,s,c); }

void main() {
    vec2 uv = (v_uv * 2.0 - 1.0);
    uv.x   *= uResolution.x / uResolution.y;
    float px = 2.0 / min(uResolution.x, uResolution.y);

    float t   = uTime * (0.2 + u_p8 * 0.6);
    float rot = u_p5 * 3.14159;
    float sz  = 0.12 + u_p2 * 0.3;
    float k   = 0.01 + u_p3 * 0.2;

    // Shape A — offset right, rotated by CC5
    vec2 uvA = rot2( rot + t*0.2) * (uv - vec2(0.3, 0.0));
    // Shape B — offset left, counter-rotated
    vec2 uvB = rot2(-rot - t*0.2) * (uv + vec2(0.3, 0.0));

    // CC1 selects shape type for A
    float dA;
    if      (u_p1 < 0.33) dA = sdCircle  (uvA, sz);
    else if (u_p1 < 0.66) dA = sdBox     (uvA, vec2(sz * 0.85));
    else                   dA = sdRoundBox(uvA, vec2(sz * 0.75), sz * 0.2);

    float dB = sdCircle(uvB, sz * 0.9);

    // CC4 selects combination method
    float d;
    if      (u_p4 < 0.25) d = min(dA, dB);        // union
    else if (u_p4 < 0.50) d = max(dA, dB);        // intersection
    else if (u_p4 < 0.75) d = max(dA, -dB);       // subtraction (A minus B)
    else                   d = smin(dA, dB, k);    // smooth union (CC3 = blend radius)

    float fill = 1.0 - smoothstep(-px, px, d);
    float glow = exp(-max(d,0.0) * (6.0 - u_p6*5.5)) * u_p6;
    float rim  = 1.0 - smoothstep(0.0, px*3.0, abs(d) - px);

    vec3 fg = 0.55 + 0.45 * cos(u_p7 * 6.28318 + vec3(0, 2.1, 4.2));
    vec3 bg = vec3(0.04, 0.04, 0.09);

    vec3 col = bg + fg*(fill + glow) + vec3(rim*fill*0.4);
    fragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
