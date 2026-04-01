// scene.frag — raymarched feedback-like visuals via temporal accumulation
//
// Creates the visual impression of feedback using:
//   - Motion blur across multiple time samples per pixel (5 samples)
//   - Domain warping that accumulates with uTime
//   - Hue rotation tied to frame depth
//   - Orbiting geometry with trailing persistence via blur
//
// CC1 = camera orbit speed
// CC2 = camera distance
// CC3 = trail depth (motion blur samples weight)
// CC4 = hue drift
// CC5 = scene complexity
// CC6 = light temperature
// CC7 = zoom/pulse
// CC8 = animation speed

uniform float u_orbit;
uniform float u_distance;
uniform float u_trails;
uniform float u_hue_drift;
uniform float u_complexity;
uniform float u_light_temp;
uniform float u_pulse;
uniform float u_speed;

#define PI  3.14159265359
#define TAU 6.28318530718

mat2 rot2(float a) { float c=cos(a),s=sin(a); return mat2(c,-s,s,c); }

// ── SDF scene ─────────────────────────────────────────────────────────────
float sdSphere(vec3 p, float r) { return length(p)-r; }
float sdTorus(vec3 p, vec2 t) { vec2 q=vec2(length(p.xz)-t.x,p.y); return length(q)-t.y; }
float sdBox(vec3 p, vec3 b) { vec3 q=abs(p)-b; return length(max(q,0.))+min(max(q.x,max(q.y,q.z)),0.); }
float smin(float a, float b, float k) {
    float h=clamp(.5+.5*(b-a)/k,0.,1.); return mix(b,a,h)-k*h*(1.-h);
}

float scene(vec3 p, float t) {
    float morph  = u_complexity;
    float spd    = t * (0.25 + u_speed * 0.8);

    // Rotate scene
    p.xz *= rot2(spd * 0.3);
    p.xy *= rot2(spd * 0.15 * morph);

    // Central sphere — pulses
    float dSphere = sdSphere(p, 0.8 + 0.15 * sin(spd * 1.7));

    // Three orbiting tori at different axes
    vec3 pt1 = p; pt1.xz *= rot2(spd * 0.8);
    float dT1 = sdTorus(pt1, vec2(1.4, 0.18 + morph * 0.08));

    vec3 pt2 = p; pt2.xy *= rot2(spd * 0.6 + PI * 0.5);
    float dT2 = sdTorus(pt2, vec2(1.2, 0.14));

    vec3 pt3 = p; pt3.yz *= rot2(spd * 0.4 + PI * 0.25);
    float dT3 = sdTorus(pt3, vec2(1.6 + morph * 0.3, 0.12));

    // Domain-warped floating boxes
    vec3 pb = fract(p * 0.35 + 0.5) - 0.5;
    float dBox = sdBox(pb / 0.35, vec3(0.08 + morph * 0.06));
    dBox = dBox * 0.35;

    float d = smin(dSphere, dT1, 0.25);
    d = smin(d, dT2, 0.2 + morph * 0.15);
    d = smin(d, dT3, 0.18);
    d = smin(d, dBox, 0.12 + morph * 0.1);
    return d;
}

vec3 calcNormal(vec3 p, float t) {
    float e = 0.001;
    return normalize(vec3(
        scene(p+vec3(e,0,0),t)-scene(p-vec3(e,0,0),t),
        scene(p+vec3(0,e,0),t)-scene(p-vec3(0,e,0),t),
        scene(p+vec3(0,0,e),t)-scene(p-vec3(0,0,e),t)));
}

float softShadow(vec3 ro, vec3 rd, float t, float k) {
    float res=1., d=0.01;
    for(int i=0;i<16;i++){
        float h=scene(ro+rd*d,t);
        if(h<0.001) return 0.;
        res=min(res, k*h/d);
        d+=clamp(h,0.02,0.2);
        if(d>6.) break;
    }
    return clamp(res,0.,1.);
}

// ── Shade one sample ───────────────────────────────────────────────────────
vec3 shade(vec2 uv, float t, float hue_phase) {
    float angle = u_orbit * TAU + t * 0.1;
    float elev  = 0.4 + u_pulse * 0.3 * sin(t * 0.7);
    float dist  = mix(2.5, 10.0, u_distance);

    // Pulse zoom
    dist *= 1.0 + u_pulse * 0.08 * sin(t * 2.1);

    vec3 camPos = vec3(cos(angle)*cos(elev), sin(elev), sin(angle)*cos(elev)) * dist;
    vec3 camFwd = normalize(-camPos);
    vec3 camRight = normalize(cross(camFwd, vec3(0,1,0)));
    vec3 camUp = cross(camRight, camFwd);
    vec3 rd = normalize(uv.x * camRight + uv.y * camUp + 1.8 * camFwd);
    vec3 ro = camPos;

    // March
    float tt = 0.0;
    float mat = -1.0;
    for(int i=0;i<96;i++){
        float d = scene(ro+rd*tt, t);
        if(d<0.001){ mat=1.; break; }
        if(tt>25.) break;
        tt+=d;
    }

    vec3 col;
    if(mat>0.){
        vec3 p = ro+rd*tt;
        vec3 n = calcNormal(p, t);
        vec3 lp = vec3(3.,5.,2.) + 2.*vec3(cos(t*0.3), 0., sin(t*0.3));
        vec3 ld = normalize(lp-p);

        float diff = max(dot(n,ld),0.);
        float shad = softShadow(p+n*0.01, ld, t, 8.);
        float ao   = 0.5 + 0.5*dot(n, normalize(p));

        float temp = u_light_temp;
        vec3 lightCol = mix(vec3(1.0,0.7,0.4), vec3(0.5,0.8,1.0), temp) * 2.0;

        // Hue-shifted surface
        vec3 hue = 0.5 + 0.5*cos(hue_phase + vec3(0, 2.1, 4.2));
        vec3 surf = mix(hue, vec3(0.9,0.95,1.0), 0.15);

        float spec = pow(max(dot(reflect(-ld,n),-rd),0.), 48.0);

        col  = surf * (0.05 * ao + lightCol * diff * shad);
        col += lightCol * spec * shad * 0.5;

        // Subsurface
        col += surf * pow(max(dot(-rd,n),0.),2.) * 0.1;

        // Fog
        float fog = 1.0 - exp(-tt * 0.05);
        vec3 sky = mix(vec3(0.02,0.03,0.08), vec3(0.08,0.05,0.15), temp);
        col = mix(col, sky, fog);

    } else {
        // Sky
        float horizon = smoothstep(-0.1, 0.4, rd.y);
        vec3 skyLow  = mix(vec3(0.03,0.03,0.1), vec3(0.1,0.06,0.18), u_light_temp);
        vec3 skyHigh = mix(vec3(0.01,0.02,0.12), vec3(0.04,0.08,0.22), u_light_temp);
        col = mix(skyLow, skyHigh, horizon);
        // Stars
        float star = step(0.997, fract(sin(dot(floor(rd*350.), vec3(127.1,311.7,74.3)))*43758.5));
        col += star * 0.5;
    }
    return col;
}

// ── Main: motion blur = temporal feedback illusion ─────────────────────────
void main() {
    vec2 uv = (vec2(v_uv.x, 1.0 - v_uv.y) * 2.0 - 1.0)
              * vec2(uResolution.x / uResolution.y, 1.0);

    // Hue drifts with time — creates colour trails as objects move
    float hue_base = uTime * (u_hue_drift - 0.5) * 0.4
                   + u_hue_drift * TAU;

    // Motion blur: sample across recent time window
    // Higher u_trails = wider time window = longer perceived trails
    float trail_window = mix(0.0, 0.35, u_trails);
    int   samples      = 5;
    vec3  acc          = vec3(0.0);
    float weight_sum   = 0.0;

    for(int i = 0; i < samples; i++) {
        float frac   = float(i) / float(samples - 1);        // 0..1
        float t_off  = -trail_window * frac;                  // negative = past
        float t      = uTime + t_off;

        // Weight: current frame full weight, older frames decay
        float w = exp(-frac * 3.0 * (1.0 - u_trails * 0.5));

        // Hue phase shifts per sample — older samples have rotated hue
        float hue_phase = hue_base + t_off * 2.0;

        acc        += shade(uv, t, hue_phase) * w;
        weight_sum += w;
    }
    vec3 col = acc / weight_sum;

    // ACES tone map
    col = col * (2.51*col + 0.03) / (col * (2.43*col + 0.59) + 0.14);
    col = pow(clamp(col, 0., 1.), vec3(0.4545));

    // Vignette
    float vig = 1.0 - dot(v_uv - 0.5, (v_uv - 0.5) * 1.6);
    col *= vig;

    fragColor = vec4(col, 1.0);
}
