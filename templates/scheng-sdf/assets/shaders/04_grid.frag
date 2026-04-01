// 04_grid.frag — Grid of SDF shapes with wave deformation
// Regular tiled geometry — great for abstract patterns and key work
//
// CC1=speed  CC2=grid density  CC3=shape morph (circle→box→tri)  CC4=hue
// CC5=wave X  CC6=wave Y  CC7=bg (0=key)  CC8=size pulse

uniform float u_p1;
uniform float u_p2;
uniform float u_p3;
uniform float u_p4;
uniform float u_p5;
uniform float u_p6;
uniform float u_p7;
uniform float u_p8;

float sdCircle(vec2 p, float r) { return length(p)-r; }
float sdBox(vec2 p, vec2 b) { vec2 d=abs(p)-b; return length(max(d,0.))+min(max(d.x,d.y),0.); }
float sdTri(vec2 p, float r) {
    const float k=1.732; p.x=abs(p.x)-r; p.y=p.y+r/k;
    if(p.x+k*p.y>0.) p=vec2(p.x-k*p.y,-k*p.x-p.y)/2.;
    p.x-=clamp(p.x,-2.*r,0.); return -length(p)*sign(p.y);
}

float shapeAt(vec2 p, float r, float morph) {
    float dc = sdCircle(p, r);
    float db = sdBox(p, vec2(r*0.85));
    float dt = sdTri(p, r*1.1);
    // Smoothly morph between the three
    if      (morph < 0.5) return mix(dc, db, morph * 2.0);
    else                  return mix(db, dt, (morph - 0.5) * 2.0);
}

void main() {
    vec2 uv = v_uv * 2.0 - 1.0;
    uv.x   *= uResolution.x / uResolution.y;
    float t  = uTime * (0.15 + u_p1 * 0.8);
    float px = 2.0 / min(uResolution.x, uResolution.y);

    // Grid density: CC2=0 → 3×3, CC2=1 → 10×10
    float density = 3.0 + u_p2 * 7.0;
    vec2  cell    = fract(uv * density * 0.5 + 0.5) - 0.5;
    vec2  cellId  = floor(uv * density * 0.5 + 0.5);

    // Wave deformation per cell
    float wx = sin(cellId.x * 0.8 + t * (0.5 + u_p5 * 1.5)) * u_p5 * 0.4;
    float wy = sin(cellId.y * 0.8 + t * (0.4 + u_p6 * 1.2)) * u_p6 * 0.4;
    vec2  warp = vec2(wx, wy);

    // Size pulse per cell (phase offset by cell ID)
    float phase = sin(cellId.x * 1.3 + cellId.y * 1.7 + t * 1.5);
    float sz = (0.25 + u_p8 * 0.2 * phase) / density;

    float d = shapeAt(cell + warp, sz, u_p3);

    float fill = 1.0 - smoothstep(-px, px, d);
    float glow = exp(-max(d,0.) * (density * 3.0)) * 0.4;

    // Per-cell hue phase
    float hueOff = (cellId.x + cellId.y) * 0.4 + t * 0.2;
    vec3 fg = 0.55 + 0.45*cos(u_p4*6.28318 + hueOff + vec3(0,2.1,4.2));

    vec3 bg = mix(vec3(0.0), 0.06*(0.5+0.5*cos(u_p4*6.28318+vec3(0.5,2.6,4.7))), u_p7);
    bg = max(bg, 0.0);

    vec3 col = bg + fg*(fill + glow);
    col = col/(1.0+col);
    col = pow(clamp(col, 0.0, 1.0), vec3(0.4545));
    fragColor = vec4(col, 1.0);
}
