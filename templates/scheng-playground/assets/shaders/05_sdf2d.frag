// 05_sdf2d.frag — 2D signed distance fields
// Demonstrates smooth shape blending and SDF operations.
// This is the 2D equivalent of the raymarcher's geometry.
//
// u_p1 = animation speed
// u_p2 = shape morph
// u_p3 = smooth blend radius
// u_p4 = color

float sdCircle(vec2 p, float r) { return length(p) - r; }
float sdBox(vec2 p, vec2 b) {
    vec2 d = abs(p) - b;
    return length(max(d, 0.0)) + min(max(d.x, d.y), 0.0);
}
float sdEquilateralTriangle(vec2 p, float r) {
    const float k = 1.732;
    p.x = abs(p.x) - r;
    p.y = p.y + r / k;
    if (p.x + k * p.y > 0.0) p = vec2(p.x - k * p.y, -k * p.x - p.y) / 2.0;
    p.x -= clamp(p.x, -2.0 * r, 0.0);
    return -length(p) * sign(p.y);
}
float smin(float a, float b, float k) {
    float h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
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
    vec2  uv  = (v_uv * 2.0 - 1.0);
    uv.x     *= uResolution.x / uResolution.y;
    float t   = uTime * (0.4 + u_p1 * 1.0);

    // Three shapes orbiting center
    float r1  = 0.22 + 0.05 * sin(t);
    float r2  = 0.18 + 0.04 * cos(t * 1.3);
    float orb = 0.35 + u_p2 * 0.2;

    vec2  p1  = uv - vec2(cos(t)        * orb, sin(t)        * orb);
    vec2  p2  = uv - vec2(cos(t + 2.09) * orb, sin(t + 2.09) * orb);
    vec2  p3  = uv - vec2(cos(t + 4.19) * orb, sin(t + 4.19) * orb);

    float d1  = sdCircle(p1, r1);
    float d2  = sdBox(p2, vec2(r2));
    float d3  = sdEquilateralTriangle(p3, r2 * 1.2);

    float k   = 0.05 + u_p3 * 0.25;
    float d   = smin(smin(d1, d2, k), d3, k);

    // Interior, border, exterior
    vec3  colA = mix(vec3(0.05, 0.3, 0.8), vec3(0.8, 0.2, 0.5), u_p4);
    vec3  colB = vec3(1.0);
    vec3  colC = vec3(0.04, 0.04, 0.08);

    vec3  col  = colC;
    col = mix(col, colA, 1.0 - smoothstep(0.0, 0.004, d));       // fill
    col = mix(col, colB, 1.0 - smoothstep(0.0, 0.004, abs(d) - 0.003)); // border

    fragColor = vec4(col, 1.0);
}
