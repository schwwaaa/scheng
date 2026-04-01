// ═══════════════════════════════════════════════════════════════════════════
// LESSON 05 — Raymarching: 3D in a Fragment Shader
// How to render solid 3D objects without a 3D API.
// ═══════════════════════════════════════════════════════════════════════════
//
// IDEA: for each pixel, shoot a ray from the camera through it into the scene.
// March the ray forward step by step until it hits something.
//
// How far can we safely step? The SDF tells us:
//   d = scene(current_position)   ← distance to nearest surface
//   If d < 0.001: we've hit something. Stop and shade it.
//   Otherwise: step forward exactly d units. We won't miss anything.
//
// This is called "sphere tracing." Safe, fast, produces smooth surfaces.
//
// NORMALS: approximate the surface normal numerically using the gradient
// of the SDF. Small nudges in XYZ → rate of change = normal direction.
//
// LIGHTING: Blinn-Phong. Three components:
//   Ambient:  constant low-level light so nothing is pitch black
//   Diffuse:  brightest where surface faces the light (dot product)
//   Specular: mirror highlight (reflected ray toward camera)
//
// ─────────────────────────────────────────────────────────────────────────
// CC1 = camera orbit speed    CC2 = camera distance
// CC3 = scene complexity      CC4 = light warm/cool
// CC5 = specular intensity    CC6 = fog density
// CC7 = surface colour        CC8 = shape animation speed

uniform float u_p1;
uniform float u_p2;
uniform float u_p3;
uniform float u_p4;
uniform float u_p5;
uniform float u_p6;
uniform float u_p7;
uniform float u_p8;

// ── 3D SDF library ───────────────────────────────────────────────────────
float sdSphere(vec3 p, float r) { return length(p) - r; }

float sdTorus(vec3 p, float R, float r) {
    // R = major radius (ring size), r = minor radius (tube thickness)
    return length(vec2(length(p.xz) - R, p.y)) - r;
}

float sdBox(vec3 p, vec3 b) {
    vec3 q = abs(p) - b;
    return length(max(q, 0.0)) + min(max(q.x, max(q.y, q.z)), 0.0);
}

float smin(float a, float b, float k) {
    float h = clamp(0.5+0.5*(b-a)/k, 0.0, 1.0);
    return mix(b,a,h)-k*h*(1.0-h);
}

// ── Scene definition ─────────────────────────────────────────────────────
// This function returns the distance to the nearest surface at point p.
float scene(vec3 p) {
    float t = uTime * (0.3 + u_p8 * 0.5);

    // Central pulsing sphere
    float sphere = sdSphere(p, 0.35 + 0.06*sin(t*1.7));

    // Torus orbiting around it
    vec3 tp = p;
    tp.xz = mat2(cos(t), -sin(t), sin(t), cos(t)) * tp.xz;
    float torus = sdTorus(tp, 0.65, 0.12);

    float d = smin(sphere, torus, 0.1);

    // Additional boxes, enabled by CC3
    int n = int(u_p3 * 4.0);
    for (int i = 0; i < 4; i++) {
        if (i >= n) break;
        float a  = t * 0.9 + float(i) * 6.28318 / 4.0;
        vec3  bp = p - vec3(cos(a)*0.55, sin(a*0.7)*0.2, sin(a)*0.55);
        d = smin(d, sdBox(bp, vec3(0.07)), 0.06);
    }

    return d;
}

// ── Surface normal via finite differences ─────────────────────────────────
vec3 calcNormal(vec3 p) {
    float e = 0.001;
    return normalize(vec3(
        scene(p + vec3(e,0,0)) - scene(p - vec3(e,0,0)),
        scene(p + vec3(0,e,0)) - scene(p - vec3(0,e,0)),
        scene(p + vec3(0,0,e)) - scene(p - vec3(0,0,e))
    ));
}

void main() {
    // ── Camera ────────────────────────────────────────────────────────────
    // Y-flip: scheng renders with Y=0 at bottom, display expects Y=0 at top
    vec2 uv = (vec2(v_uv.x, 1.0 - v_uv.y) * 2.0 - 1.0);
    uv.x   *= uResolution.x / uResolution.y;

    float angle = uTime * (0.1 + u_p1 * 0.4);
    float dist  = 1.8 + u_p2 * 2.5;
    vec3  ro    = vec3(cos(angle)*dist, 0.7, sin(angle)*dist);
    vec3  fwd   = normalize(-ro);
    vec3  right = normalize(cross(fwd, vec3(0,1,0)));
    vec3  up    = cross(right, fwd);
    vec3  rd    = normalize(uv.x*right + uv.y*up + 1.8*fwd);

    // ── March ─────────────────────────────────────────────────────────────
    float t = 0.0;
    bool  hit = false;
    for (int i = 0; i < 64; i++) {
        float d = scene(ro + rd * t);
        if (d < 0.001) { hit = true; break; }
        if (t > 20.0)  { break; }
        t += d;  // safe to jump exactly d — sphere tracing guarantee
    }

    // ── Shade ─────────────────────────────────────────────────────────────
    vec3 col;
    if (hit) {
        vec3 p  = ro + rd * t;
        vec3 n  = calcNormal(p);

        // Light position and direction
        vec3 lp = vec3(2.0, 3.0, 1.5);
        vec3 ld = normalize(lp - p);

        float diff = max(dot(n, ld), 0.0);
        float spec = pow(max(dot(reflect(-ld, n), -rd), 0.0), 24.0) * u_p5;

        // Warm/cool light from CC4
        vec3 light = mix(vec3(1.0,0.75,0.4), vec3(0.5,0.8,1.0), u_p4) * 1.8;

        // Surface colour from CC7, shaded by normal direction
        vec3 surf = 0.5 + 0.5*cos(u_p7*6.28318 + n*2.0 + vec3(0,2.1,4.2));

        col  = surf * (0.04 + diff * light);
        col += light * spec;

        // Fog from CC6
        float fog = 1.0 - exp(-t * u_p6 * 0.25);
        col = mix(col, vec3(0.04,0.04,0.1), fog);

    } else {
        // Background sky
        float h = rd.y * 0.5 + 0.5;
        col = mix(vec3(0.04,0.04,0.1), vec3(0.08,0.06,0.18), h);
    }

    // Gamma correction: linear rendering → display (sRGB)
    col = pow(clamp(col, 0.0, 1.0), vec3(0.4545));
    fragColor = vec4(col, 1.0);
}
