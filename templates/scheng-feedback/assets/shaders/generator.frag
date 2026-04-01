// generator.frag — foreground content source
//
// Generates the visual content that feeds into the feedback/luma-key pass.
// Bright areas of this shader will KEY THROUGH the feedback buffer.
// Dark areas will let the feedback trail accumulate and persist.
//
// Hot-reload this shader to change what you're feeding into the feedback.
//
// CC1 = animation speed
// CC2 = shape scale / size
// CC3 = complexity (layers / orbit count)
// CC4 = palette / hue

uniform float u_speed;     // CC1
uniform float u_scale;     // CC2
uniform float u_complexity; // CC3
uniform float u_hue;       // CC4

// SDF helpers
float sdCircle(vec2 p, float r) { return length(p) - r; }
float sdBox(vec2 p, vec2 b) {
    vec2 d = abs(p) - b;
    return length(max(d, 0.0)) + min(max(d.x, d.y), 0.0);
}
float sdTri(vec2 p, float r) {
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

void main() {
    vec2 uv   = v_uv * 2.0 - 1.0;
    uv.x     *= uResolution.x / uResolution.y;
    float t   = uTime * (0.3 + u_speed * 1.2);
    float sc  = 0.18 + u_scale * 0.22;
    int   N   = 2 + int(u_complexity * 4.0);  // 2–6 shapes

    // Multiple orbiting SDF shapes
    float d = 1e9;
    for (int i = 0; i < 6; i++) {
        if (i >= N) break;
        float a = t + float(i) * 6.28318 / float(N);
        float orb = 0.3 + u_scale * 0.2;
        vec2  center = vec2(cos(a), sin(a)) * orb;

        // Alternate shape types
        float shape;
        if (i == 0 || i == 3) {
            shape = sdCircle(uv - center, sc);
        } else if (i == 1 || i == 4) {
            shape = sdBox(uv - center, vec2(sc * 0.85));
        } else {
            shape = sdTri(uv - center, sc * 1.1);
        }
        d = smin(d, shape, 0.06 + u_complexity * 0.12);
    }

    // Centre pulsing sphere
    float pulse = sdCircle(uv, 0.08 + 0.04 * sin(t * 2.3));
    d = smin(d, pulse, 0.05);

    // Shape fill and glow
    float fill  = 1.0 - smoothstep(-0.006, 0.006, d);
    float glow  = exp(-max(d, 0.0) * 10.0) * 0.5;
    float edge  = 1.0 - smoothstep(0.0, 0.004, abs(d) - 0.003);

    // Colour from hue param
    vec3  hue_col = 0.55 + 0.45 * cos(u_hue * 6.28 + float(uFrame) * 0.001 + vec3(0.0, 2.1, 4.2));
    vec3  col     = hue_col * (fill + glow) + vec3(edge);

    // Keep background dark — important for luma key
    // Bright = keys through feedback, dark = lets feedback accumulate
    fragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
