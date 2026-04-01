// 07_lines.frag — interference / moiré lines
// Additive sine-wave interference — very musical, responds well to MIDI.
//
// u_p1 = speed
// u_p2 = line frequency
// u_p3 = rotation
// u_p4 = second frequency
// u_p5 = color balance
// u_p6 = brightness

uniform float u_p1;
uniform float u_p2;
uniform float u_p3;
uniform float u_p4;
uniform float u_p5;
uniform float u_p6;
uniform float u_p7;
uniform float u_p8;
void main() {
    float t  = uTime * (0.3 + u_p1 * 1.5);
    vec2  uv = v_uv * 2.0 - 1.0;
    uv.x    *= uResolution.x / uResolution.y;

    float angle = u_p3 * 3.14159;
    vec2  dir1  = vec2(cos(angle), sin(angle));
    vec2  dir2  = vec2(cos(angle + 1.5708), sin(angle + 1.5708));

    float f1 = 8.0  + u_p2 * 40.0;
    float f2 = 6.0  + u_p4 * 30.0;

    float v1 = sin(dot(uv, dir1) * f1 + t) * 0.5 + 0.5;
    float v2 = sin(dot(uv, dir2) * f2 - t * 1.3) * 0.5 + 0.5;
    float v3 = sin(length(uv) * (f1 * 0.5) - t * 0.8) * 0.5 + 0.5;

    float pattern = v1 * v2 * (0.5 + 0.5 * v3);
    pattern       = pow(pattern, 1.5 - u_p6 * 1.0);

    vec3  col = mix(
        vec3(pattern * 0.1, pattern * 0.3, pattern),
        vec3(pattern, pattern * 0.3, pattern * 0.1),
        u_p5
    );

    fragColor = vec4(col, 1.0);
}
