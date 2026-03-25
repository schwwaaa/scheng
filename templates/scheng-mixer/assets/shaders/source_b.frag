// source_b.frag — Geometric tunnel
// Hard-edged rotating geometric tunnel with scanlines

void main() {
    vec2 uv = v_uv * 2.0 - 1.0;
    uv.x *= uResolution.x / uResolution.y;

    float t = uTime * 0.5;

    // Polar coordinates
    float r = length(uv);
    float a = atan(uv.y, uv.x);

    // Tunnel depth rings
    float rings = mod(1.0 / (r + 0.1) - t * 2.0, 1.0);

    // Rotating sectors
    float sectors = step(0.5, mod((a + t) / 3.14159 * 3.0, 1.0));

    // Combine
    float pattern = rings * sectors;

    // Hard scanlines
    float scan = step(0.5, mod(v_uv.y * uResolution.y / 4.0, 1.0));
    pattern *= 0.7 + 0.3 * scan;

    // Orange/gold palette
    vec3 col = vec3(
        pattern,
        pattern * 0.6,
        pattern * 0.05
    );

    // Vignette
    col *= 1.0 - smoothstep(0.5, 1.2, r);

    fragColor = vec4(col, 1.0);
}
