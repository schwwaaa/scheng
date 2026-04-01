// 06_polygons.frag — Rotating regular polygons, nested and orbiting
// Sharp-edged geometry that keys cleanly. CC7=0 → pure key output.
//
// CC1=speed  CC2=sides (3–10)  CC3=nest count  CC4=hue
// CC5=orbit count  CC6=size  CC7=bg (0=key)  CC8=counter-rotation

uniform float u_p1;
uniform float u_p2;
uniform float u_p3;
uniform float u_p4;
uniform float u_p5;
uniform float u_p6;
uniform float u_p7;
uniform float u_p8;

float sdNgon(vec2 p, float r, float n) {
    float a = atan(p.y, p.x) + 3.14159/n;
    float b = 6.28318 / n;
    return cos(floor(0.5 + a/b)*b - a) * length(p) - r;
}
float smin(float a, float b, float k) {
    float h=clamp(.5+.5*(b-a)/k,0.,1.); return mix(b,a,h)-k*h*(1.-h);
}
mat2 rot2(float a) { float c=cos(a),s=sin(a); return mat2(c,-s,s,c); }

void main() {
    vec2 uv = (v_uv*2.0-1.0);
    uv.x   *= uResolution.x / uResolution.y;
    float px = 2.0 / min(uResolution.x, uResolution.y);
    float t  = uTime * (0.2 + u_p1 * 0.9);

    float sides = 3.0 + floor(u_p2 * 7.0);   // 3–10
    int   nst   = 1 + int(u_p3 * 3.0);        // nested count
    float sz    = 0.15 + u_p6 * 0.35;
    float crot  = t * (0.4 + u_p8 * 0.8);

    // Nested central polygons
    float d = 1e9;
    for (int i = 0; i < 4; i++) {
        if (i >= nst) break;
        float fi  = float(i);
        float rr  = sz * (1.0 - fi * 0.22);
        float rot = crot * (i % 2 == 0 ? 1.0 : -1.0) + fi * 0.5;
        vec2  puv = rot2(rot) * uv;
        float dng = sdNgon(puv, rr, sides);
        // Alternating: outline vs fill
        float dn  = (i % 2 == 0) ? abs(dng) - px*2.5 : dng;
        d = smin(d, dn, 0.02);
    }

    // Orbiting smaller polygons
    int   norb = 1 + int(u_p5 * 4.0);
    float orb  = sz * 1.5 + u_p5 * 0.2;
    for (int i = 0; i < 5; i++) {
        if (i >= norb) break;
        float a   = t * 0.6 + float(i) * 6.28318 / float(norb);
        vec2  c   = vec2(cos(a), sin(a)) * orb;
        vec2  puv = rot2(-crot) * (uv - c);
        d = smin(d, sdNgon(puv, sz*0.28, sides), 0.02);
    }

    float fill = 1.0 - smoothstep(-px, px, d);
    float glow = exp(-max(d,0.)*10.0)*0.4;
    float rim  = 1.0 - smoothstep(0., px*3., abs(d) - px);

    // Hue shifts with rotation angle
    float r = length(uv);
    vec3 fg = 0.55 + 0.45*cos(u_p4*6.28318 + r*2.0 - t*0.3 + vec3(0,2.1,4.2));

    vec3 bg = mix(vec3(0.0), 0.08*(0.5+0.5*cos(u_p4*6.28318+3.14+vec3(0,2.1,4.2))), u_p7);
    bg = max(bg, 0.0);

    vec3 col = bg + fg*(fill + glow) + vec3(1.0)*rim*fill*0.25;
    col = col/(1.0+col);
    col = pow(clamp(col, 0.0, 1.0), vec3(0.4545));
    fragColor = vec4(col, 1.0);
}
