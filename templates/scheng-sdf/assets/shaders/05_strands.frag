// 05_strands.frag — Flowing SDF line strands
// Sine-warped lines with smooth AA. Good for textural key layers.
//
// CC1=speed  CC2=strand count  CC3=strand width  CC4=hue
// CC5=wave amplitude  CC6=wave frequency  CC7=bg (0=key)  CC8=skew

uniform float u_p1;
uniform float u_p2;
uniform float u_p3;
uniform float u_p4;
uniform float u_p5;
uniform float u_p6;
uniform float u_p7;
uniform float u_p8;

void main() {
    vec2 uv = v_uv * 2.0 - 1.0;
    uv.x   *= uResolution.x / uResolution.y;
    float t  = uTime * (0.2 + u_p1 * 1.4);
    float px = 2.0 / min(uResolution.x, uResolution.y);

    int   N   = 2 + int(u_p2 * 8.0);
    float sw  = 0.006 + u_p3 * 0.025;
    float amp = u_p5 * 0.5;
    float frq = 1.5 + u_p6 * 5.0;
    float skw = (u_p8 - 0.5) * 1.5;

    vec3 col = vec3(0.0);

    for (int i = 0; i < 10; i++) {
        if (i >= N) break;
        float fi = float(i) / float(N);
        float y0 = mix(-0.85, 0.85, fi);

        // Strand centre line: a sine wave travelling in time
        float wave = amp * sin(uv.x * frq + t + fi * 2.1) + skw * uv.x;
        float dy   = uv.y - y0 - wave;
        float d    = abs(dy) - sw;

        float fill = 1.0 - smoothstep(-px, px, d);
        float glow = exp(-max(d,0.) * 18.0) * 0.5;

        // Per-strand hue offset
        float hue  = u_p4 * 6.28318 + fi * 4.2 + t * 0.1;
        vec3  fg   = 0.5 + 0.5*cos(hue + vec3(0, 2.1, 4.2));

        col += fg * (fill + glow);
    }

    col = col / (1.0 + col);
    col = pow(clamp(col, 0.0, 1.0), vec3(0.4545));

    // Background blend
    vec3 bg = mix(vec3(0.0), 0.07*(0.5+0.5*cos(u_p4*6.28318+vec3(1.0,3.1,5.2))), u_p7);
    bg = max(bg, 0.0);
    col = col + bg * (1.0 - col);

    fragColor = vec4(col, 1.0);
}
