// source_a.frag — Plasma electric field
// Layered sine-wave plasma with interference rings

void main() {
    vec2 uv = v_uv * 2.0 - 1.0;
    uv.x *= uResolution.x / uResolution.y;

    float t = uTime * 0.8;

    float v = 0.0;
    v += sin(uv.x * 4.0 + t);
    v += sin(uv.y * 4.0 + t * 1.3);
    v += sin((uv.x + uv.y) * 4.0 + t * 0.7);
    float cx = uv.x + 0.5 * sin(t * 0.3);
    float cy = uv.y + 0.5 * cos(t * 0.2);
    v += sin(sqrt(cx*cx + cy*cy) * 8.0 - t * 2.0);

    vec3 col;
    col.r = sin(v * 3.14159) * 0.5 + 0.5;
    col.g = sin(v * 3.14159 + 2.094) * 0.5 + 0.5;
    col.b = sin(v * 3.14159 + 4.189) * 0.5 + 0.5;
    col = pow(col, vec3(1.5));

    fragColor = vec4(col, 1.0);
}
