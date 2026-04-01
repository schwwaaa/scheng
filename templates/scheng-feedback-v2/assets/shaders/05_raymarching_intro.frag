// ═══════════════════════════════════════════════════════════════════════════
// LESSON 05 — Introduction to Raymarching
// How to render 3D scenes in a fragment shader.
// ═══════════════════════════════════════════════════════════════════════════
//
// Raymarching is how we draw 3D objects in a 2D shader:
//
//   1. SETUP: For each pixel, compute a ray direction from camera through it.
//
//   2. MARCH: Step the ray forward. At each step, ask:
//      "How far am I from the nearest surface?" (the SDF value)
//      If that distance is very small, we've hit something.
//      If it's large, we can safely jump that far without missing anything.
//
//   3. SHADE: Once we hit a surface, compute lighting from the normal.
//
// This is called "sphere tracing" — we advance by the exact safe distance
// each step, so we never overshoot. Very efficient for smooth surfaces.
//
// The 3D SDF library is just the 2D one extended to 3D:
//   float sdSphere(vec3 p, float r) { return length(p) - r; }

uniform float u_p1;  // CC1 — camera orbit speed
uniform float u_p2;  // CC2 — camera distance
uniform float u_p3;  // CC3 — light position X
uniform float u_p4;  // CC4 — light colour temperature
uniform float u_p5;  // CC5 — scene complexity
uniform float u_p6;  // CC6 — fog density
uniform float u_p7;  // CC7 — surface colour
uniform float u_p8;  // CC8 — specular intensity

// ── 3D SDF library ───────────────────────────────────────────────────────
float sdSphere(vec3 p, float r) { return length(p) - r; }

float sdBox(vec3 p, vec3 b) {
    vec3 q = abs(p) - b;
    return length(max(q, 0.0)) + min(max(q.x, max(q.y, q.z)), 0.0);
}

float sdTorus(vec3 p, float R, float r) {
    return length(vec2(length(p.xz) - R, p.y)) - r;
}

float smin(float a, float b, float k) {
    float h = clamp(0.5+0.5*(b-a)/k, 0.0, 1.0);
    return mix(b,a,h)-k*h*(1.0-h);
}

// ── Scene definition ─────────────────────────────────────────────────────
float scene(vec3 p) {
    float t = uTime * 0.4;

    // Rotating sphere
    float s = sdSphere(p - vec3(sin(t)*0.4, 0.0, cos(t)*0.4), 0.3);

    // Static torus
    float tor = sdTorus(p, 0.7, 0.15);

    // Small orbiting boxes (enabled by CC5)
    float boxes = 1e9;
    int n = 1 + int(u_p5 * 3.0);
    for (int i = 0; i < 4; i++) {
        if (i >= n) break;
        float a  = t * 1.2 + float(i) * 6.28318 / float(n);
        vec3  bp = p - vec3(cos(a)*0.5, sin(a*0.7)*0.2, sin(a)*0.5);
        boxes    = smin(boxes, sdBox(bp, vec3(0.08)), 0.05);
    }

    float d = smin(s, tor, 0.1);
    d = smin(d, boxes, 0.08);
    return d;
}

// Numerical gradient = surface normal
vec3 calcNormal(vec3 p) {
    float e = 0.001;
    return normalize(vec3(
        scene(p+vec3(e,0,0)) - scene(p-vec3(e,0,0)),
        scene(p+vec3(0,e,0)) - scene(p-vec3(0,e,0)),
        scene(p+vec3(0,0,e)) - scene(p-vec3(0,0,e))
    ));
}

void main() {
    // ── Camera setup ─────────────────────────────────────────────────────
    vec2 uv = (vec2(v_uv.x, 1.0-v_uv.y) * 2.0 - 1.0);
    uv.x   *= uResolution.x / uResolution.y;

    float angle  = uTime * (0.1 + u_p1 * 0.4);
    float dist   = 2.0 + u_p2 * 3.0;
    vec3  ro     = vec3(cos(angle)*dist, 0.8, sin(angle)*dist);  // ray origin (camera)
    vec3  target = vec3(0.0);
    vec3  fwd    = normalize(target - ro);
    vec3  right  = normalize(cross(fwd, vec3(0,1,0)));
    vec3  up     = cross(right, fwd);
    vec3  rd     = normalize(uv.x*right + uv.y*up + 1.8*fwd);   // ray direction

    // ── Raymarching loop ─────────────────────────────────────────────────
    float t = 0.0;
    bool  hit = false;
    for (int i = 0; i < 64; i++) {
        float d = scene(ro + rd * t);
        if (d < 0.001) { hit = true; break; }  // close enough = surface hit
        if (t > 20.0)  { break; }               // too far = miss
        t += d;   // safe to jump exactly d units forward
    }

    vec3 col;
    if (hit) {
        vec3 p = ro + rd * t;
        vec3 n = calcNormal(p);

        // ── Lighting ─────────────────────────────────────────────────────
        vec3 lp  = vec3((u_p3-0.5)*4.0, 2.0, 1.5);  // light position (CC3)
        vec3 ld  = normalize(lp - p);

        float diff = max(dot(n, ld), 0.0);             // diffuse
        float spec = pow(max(dot(reflect(-ld,n), -rd), 0.0), 32.0) * u_p8;

        // Light colour from CC4 (warm/cool)
        vec3 light = mix(vec3(1.0, 0.7, 0.4), vec3(0.5, 0.8, 1.0), u_p4) * 2.0;

        // Surface colour from CC7
        vec3 surf  = 0.5 + 0.5 * cos(u_p7 * 6.28 + n * 2.0 + vec3(0,2.1,4.2));

        col = surf * (0.04 + diff * light) + vec3(spec);

        // Fog from CC6
        float fog = 1.0 - exp(-t * u_p6 * 0.3);
        col = mix(col, vec3(0.05, 0.05, 0.12), fog);

    } else {
        // Sky gradient
        float h = rd.y * 0.5 + 0.5;
        col = mix(vec3(0.05,0.05,0.12), vec3(0.1,0.08,0.2), h);
    }

    // Gamma correction (linear → display)
    col = pow(clamp(col, 0.0, 1.0), vec3(0.4545));
    fragColor = vec4(col, 1.0);
}
