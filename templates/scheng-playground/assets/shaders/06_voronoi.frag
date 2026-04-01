// 06_voronoi.frag — Voronoi / cellular noise
// Each cell is owned by the nearest seed point. Seeds animate.
//
// u_p1 = animation speed
// u_p2 = cell scale
// u_p3 = edge sharpness
// u_p4 = color palette
// u_p5 = second distance blend (for patterns)

vec2 hash2(vec2 p) {
    p = vec2(dot(p, vec2(127.1, 311.7)), dot(p, vec2(269.5, 183.3)));
    return fract(sin(p) * 43758.5453);
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
    float t  = uTime * (0.2 + u_p1 * 0.6);
    float sc = 2.0 + u_p2 * 8.0;
    vec2  uv = v_uv * sc;

    vec2  cell = floor(uv);
    vec2  frac = fract(uv);

    float d1 = 8.0, d2 = 8.0;
    vec2  nearest;

    for (int y = -1; y <= 1; y++) {
        for (int x = -1; x <= 1; x++) {
            vec2  n    = vec2(float(x), float(y));
            vec2  seed = hash2(cell + n);
            // Animate seeds
            vec2  anim = sin(seed * 6.28 + t * vec2(1.0, 0.8)) * 0.5 + 0.5;
            vec2  r    = n + anim - frac;
            float dist = dot(r, r);
            if (dist < d1) { d2 = d1; d1 = dist; nearest = seed; }
            else if (dist < d2) { d2 = dist; }
        }
    }

    d1 = sqrt(d1); d2 = sqrt(d2);
    float edge  = d2 - d1;
    float sharp = 1.0 - smoothstep(0.0, 0.03 + (1.0 - u_p3) * 0.1, edge);

    // Cell color from seed
    vec3  cellCol = 0.5 + 0.5 * cos(6.28 * nearest.x + u_p4 * 6.28 + vec3(0, 2.1, 4.2));
    float blend   = mix(d1, d2 - d1, u_p5);

    vec3  col = cellCol * (0.3 + 0.7 * blend);
    col = mix(col, vec3(1.0), sharp * 0.8);

    fragColor = vec4(col, 1.0);
}
