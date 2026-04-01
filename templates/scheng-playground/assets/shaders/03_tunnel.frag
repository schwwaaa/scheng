// 03_tunnel.frag — rotating geometric tunnel
// Hard-edged sectors + depth rings pulling you in.
//
// u_p1 = fly speed
// u_p2 = rotation speed
// u_p3 = sector count
// u_p4 = color palette

uniform float u_p1;
uniform float u_p2;
uniform float u_p3;
uniform float u_p4;
uniform float u_p5;
uniform float u_p6;
uniform float u_p7;
uniform float u_p8;
void main() {
    vec2  uv  = v_uv * 2.0 - 1.0;
    uv.x     *= uResolution.x / uResolution.y;

    float t   = uTime;
    float r   = length(uv);
    float a   = atan(uv.y, uv.x);

    // Tunnel coords
    float depth  = 1.0 / (r + 0.05) * 0.4;
    float tunnel = mod(depth - t * (0.3 + u_p1 * 1.2), 1.0);

    // Rotating sectors
    float sectors = floor((3.0 + u_p3 * 9.0));
    float sector  = step(0.5, mod((a + t * (0.2 + u_p2 * 0.8)) / 3.14159 * sectors, 1.0));

    float pattern = tunnel * sector;

    // Scanlines
    float scan = step(0.5, mod(v_uv.y * uResolution.y / 3.0, 1.0));
    pattern   *= 0.7 + 0.3 * scan;

    // Vignette
    pattern *= 1.0 - smoothstep(0.4, 1.0, r);

    // Palette
    vec3 col;
    if (u_p4 < 0.33) {
        col = vec3(pattern, pattern * 0.5, pattern * 0.05); // amber
    } else if (u_p4 < 0.66) {
        col = vec3(pattern * 0.05, pattern * 0.6, pattern); // cyan
    } else {
        col = vec3(pattern * 0.6, pattern * 0.05, pattern); // violet
    }

    fragColor = vec4(col, 1.0);
}
