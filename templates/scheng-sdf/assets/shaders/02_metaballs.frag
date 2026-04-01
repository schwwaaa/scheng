// 02_metaballs.frag — Smooth-union metaball blobs
// Organic merging shapes. CC7=0 → black bg (key), CC7=1 → coloured
//
// CC1=speed  CC2=blob count  CC3=blend radius  CC4=hue
// CC5=spread  CC6=size  CC7=bg (0=key/black)  CC8=secondary hue

uniform float u_p1;
uniform float u_p2;
uniform float u_p3;
uniform float u_p4;
uniform float u_p5;
uniform float u_p6;
uniform float u_p7;
uniform float u_p8;

float sdCircle(vec2 p, float r) { return length(p) - r; }
float smin(float a, float b, float k) {
    float h = clamp(0.5+0.5*(b-a)/k, 0.0, 1.0);
    return mix(b, a, h) - k*h*(1.0-h);
}

void main() {
    vec2 uv = (v_uv*2.0-1.0);
    uv.x   *= uResolution.x / uResolution.y;
    float t  = uTime * (0.15 + u_p1 * 0.8);
    float px = 2.0 / min(uResolution.x, uResolution.y);

    int   N    = 2 + int(u_p2 * 5.0);
    float k    = 0.04 + u_p3 * 0.35;
    float sprd = 0.2 + u_p5 * 0.55;
    float sz   = 0.06 + u_p6 * 0.14;

    float d = 1e9;
    for (int i = 0; i < 7; i++) {
        if (i >= N) break;
        float fi = float(i);
        float a1 = t * (0.7 + fi * 0.13) + fi * 1.1;
        float a2 = t * (0.5 + fi * 0.11) * 0.7 + fi * 2.3;
        vec2  c  = vec2(cos(a1)*sprd, sin(a2)*sprd*0.7);
        d = smin(d, sdCircle(uv - c, sz + 0.02*sin(t*1.3 + fi)), k);
    }
    // Centre anchor
    d = smin(d, sdCircle(uv, sz*0.6), k*0.5);

    float fill = 1.0 - smoothstep(-px, px, d);
    float glow = exp(-max(d,0.0) * 8.0) * 0.5;

    // Dual-hue: inside vs near-surface
    float hue1 = u_p4 * 6.28318;
    float hue2 = u_p8 * 6.28318;
    float dist_n = clamp(-d / (sz*0.5), 0.0, 1.0);
    vec3  fg = mix(
        0.5 + 0.5*cos(hue1 + vec3(0,2.1,4.2)),
        0.5 + 0.5*cos(hue2 + vec3(0,2.1,4.2)),
        dist_n
    );

    vec3 bg = mix(vec3(0.0), 0.08 * (0.5+0.5*cos(hue1+3.14+vec3(0,2.1,4.2))), u_p7);
    bg = max(bg, 0.0);

    vec3 col = bg + fg*(glow + fill);
    col = col / (1.0 + col);
    col = pow(clamp(col, 0.0, 1.0), vec3(0.4545));
    fragColor = vec4(col, 1.0);
}
