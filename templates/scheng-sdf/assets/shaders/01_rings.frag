// 01_rings.frag — Concentric rings with orbital elements
// Classic VJ key tool. CC7=0 → black bg (Resolume key), CC7=1 → atmospheric bg
//
// CC1=speed  CC2=ring count  CC3=ring width  CC4=hue
// CC5=orbit radius  CC6=orbit count  CC7=bg (0=key black)  CC8=pulse depth

uniform float u_p1; // speed
uniform float u_p2; // ring count
uniform float u_p3; // ring width
uniform float u_p4; // hue
uniform float u_p5; // orbit radius
uniform float u_p6; // orbit count
uniform float u_p7; // bg (0=key/black, 1=atmospheric)
uniform float u_p8; // pulse depth

float sdCircle(vec2 p, float r) { return length(p) - r; }
float sdRing(vec2 p, float r, float w) { return abs(length(p) - r) - w; }

float smin(float a, float b, float k) {
    float h = clamp(0.5 + 0.5*(b-a)/k, 0.0, 1.0);
    return mix(b, a, h) - k*h*(1.0-h);
}

void main() {
    vec2 uv = (v_uv * 2.0 - 1.0);
    uv.x   *= uResolution.x / uResolution.y;
    float t  = uTime * (0.2 + u_p1 * 1.2);
    float px = 2.0 / min(uResolution.x, uResolution.y);

    // Rings from centre
    int   nRings = 2 + int(u_p2 * 6.0);
    float rw     = 0.008 + u_p3 * 0.04;
    float pulse  = u_p8 * 0.06 * sin(t * 2.1);
    float r      = length(uv);
    float dRings = 1e9;
    for (int i = 0; i < 8; i++) {
        if (i >= nRings) break;
        float ri = (float(i) + 1.0) * (0.18 - u_p3*0.03) + pulse * float(i+1) * 0.3;
        dRings = min(dRings, sdRing(uv, ri, rw));
    }

    // Orbiting dots
    int   nOrb = 1 + int(u_p6 * 5.0);
    float orb  = 0.12 + u_p5 * 0.5;
    float dOrb = 1e9;
    for (int i = 0; i < 6; i++) {
        if (i >= nOrb) break;
        float a  = t * 0.8 + float(i) * 6.28318 / float(nOrb);
        vec2  c  = vec2(cos(a), sin(a)) * orb;
        float sz = 0.025 + u_p3 * 0.015;
        dOrb = smin(dOrb, sdCircle(uv - c, sz), 0.03);
    }

    // Centre dot
    float dCentre = sdCircle(uv, 0.03 + pulse * 0.5);
    float d = min(min(dRings, dOrb), dCentre);

    // Smooth AA edges
    float fill = 1.0 - smoothstep(-px, px, d);
    float glow = exp(-max(d, 0.0) * 12.0) * 0.4;

    // Hue
    vec3 fg = 0.55 + 0.45 * cos(u_p4 * 6.28318 + vec3(0.0, 2.1, 4.2));

    // Background: CC7=0 → black (key mode), CC7>0 → deep colour
    vec3 bg = mix(vec3(0.0), 0.12 * cos(u_p4 * 6.28318 + 3.14 + vec3(0.0,2.1,4.2)) + 0.06, u_p7);
    bg      = max(bg, 0.0);

    vec3 col = bg + fg * glow + fg * fill;
    col = col / (1.0 + col); // Reinhard
    col = pow(clamp(col, 0.0, 1.0), vec3(0.4545));

    fragColor = vec4(col, 1.0);
}
