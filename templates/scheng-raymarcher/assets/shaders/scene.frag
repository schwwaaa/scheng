// scene.frag — 3D raymarched scene
//
// All 3D rendering happens entirely in this fragment shader.
// No vertex buffers. No mesh data. No depth buffer.
// Every pixel casts a ray into the scene and finds the surface
// by repeatedly stepping along the ray until it hits something.
//
// This is how scheng renders 3D today — full GPU precision,
// hot-reload as fast as any 2D shader.
//
// MIDI controls:
//   CC1  = camera orbit angle        (0–360°)
//   CC2  = camera elevation          (low → high)
//   CC3  = camera distance           (close → far)
//   CC4  = fog density               (clear → dense)
//   CC5  = scene complexity (morphs geometry)
//   CC6  = light color temperature   (warm → cool)
//   CC7  = reflectivity              (matte → mirror)
//   CC8  = animation speed

// ── Custom uniforms (MIDI-controlled) ─────────────────────────────────────
uniform float u_cam_angle;      // CC1  0–1 → 0–360°
uniform float u_cam_elevation;  // CC2  0–1 → low/high
uniform float u_cam_distance;   // CC3  0–1 → 2–12 units
uniform float u_fog;            // CC4  0–1
uniform float u_complexity;     // CC5  0–1
uniform float u_light_temp;     // CC6  0–1 warm–cool
uniform float u_reflectivity;   // CC7  0–1
uniform float u_speed;          // CC8  0–1

// ── Math helpers ──────────────────────────────────────────────────────────

#define PI  3.14159265359
#define TAU 6.28318530718

mat2 rot2(float a) {
    float s = sin(a), c = cos(a);
    return mat2(c, -s, s, c);
}

// ── SDF primitives ────────────────────────────────────────────────────────
// Signed distance functions — positive = outside, negative = inside.
// Distance value tells the ray how far it can safely step without
// overshooting a surface.

float sdSphere(vec3 p, float r) {
    return length(p) - r;
}

float sdBox(vec3 p, vec3 b) {
    vec3 q = abs(p) - b;
    return length(max(q, 0.0)) + min(max(q.x, max(q.y, q.z)), 0.0);
}

float sdTorus(vec3 p, vec2 t) {
    vec2 q = vec2(length(p.xz) - t.x, p.y);
    return length(q) - t.y;
}

float sdCylinder(vec3 p, float r, float h) {
    vec2 d = abs(vec2(length(p.xz), p.y)) - vec2(r, h);
    return min(max(d.x, d.y), 0.0) + length(max(d, 0.0));
}

float sdOctahedron(vec3 p, float s) {
    p = abs(p);
    return (p.x + p.y + p.z - s) * 0.57735027;
}

// ── SDF operations ────────────────────────────────────────────────────────

// Smooth union — blends two surfaces with a soft edge of radius k
float smin(float a, float b, float k) {
    float h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

// Smooth subtraction — carves b out of a with soft edge
float smax(float a, float b, float k) {
    float h = clamp(0.5 - 0.5 * (b + a) / k, 0.0, 1.0);
    return mix(a, -b, h) + k * h * (1.0 - h);
}

// ── Material IDs ──────────────────────────────────────────────────────────
// We pack material ID into the distance field via the decimal part.
// After marching, floor(d) = real distance, fract(d)*10 = material slot.
// This avoids a second SDF evaluation pass for material lookup.
#define MAT_FLOOR   1.0
#define MAT_SPHERE  2.0
#define MAT_TORUS   3.0
#define MAT_BOX     4.0

// ── Scene definition ──────────────────────────────────────────────────────

vec2 scene(vec3 p) {
    float t    = uTime * (0.3 + u_speed * 0.7);
    float morph = u_complexity;

    // Floor plane
    float dFloor = p.y + 1.5;

    // Central morphing sphere — pulses and twists with time
    vec3  ps     = p;
    ps.xz       *= rot2(t * 0.4);
    float twist  = sin(p.y * 2.0 + t) * 0.2 * morph;
    ps.xz       *= rot2(twist);
    float dSphere = sdSphere(ps, 0.9 + 0.15 * sin(t * 1.3));

    // Orbiting tori — rotate around Y and tilt based on complexity
    vec3  pt1 = p;
    pt1.xz   *= rot2(t * 0.7);
    pt1.xy   *= rot2(PI * 0.25 * morph);
    float dTorus1 = sdTorus(pt1 - vec3(1.8, 0.0, 0.0), vec2(0.6, 0.18));

    vec3  pt2 = p;
    pt2.xz   *= rot2(t * 0.7 + TAU / 3.0);
    pt2.xy   *= rot2(PI * 0.25 * morph + PI * 0.5);
    float dTorus2 = sdTorus(pt2 - vec3(1.8, 0.0, 0.0), vec2(0.6, 0.18));

    // Floating octahedra scattered around the scene
    vec3  po    = fract(p * 0.3 + 0.5) - 0.5;
    po         /= 0.3;
    float dOct  = sdOctahedron(po - vec3(0.0, sin(t + p.x + p.z) * 0.4, 0.0), 0.4);
    dOct        = dOct * 0.3 + 0.5 * morph; // scale back to world space

    // Smooth-union the central objects
    float dCore = smin(dSphere, dTorus1, 0.3);
    dCore       = smin(dCore,   dTorus2, 0.3);

    // Carve into the floor slightly
    float dAll  = smin(dFloor, dCore, 0.4);
    dAll        = smin(dAll, dOct, 0.2 + 0.3 * morph);

    // Material selection — which surface is closest?
    float mat = MAT_FLOOR;
    if (dCore < dFloor - 0.1) mat = MAT_SPHERE;
    if (abs(dTorus1) < 0.08)  mat = MAT_TORUS;
    if (abs(dTorus2) < 0.08)  mat = MAT_TORUS;

    return vec2(dAll, mat);
}

// ── Normals ───────────────────────────────────────────────────────────────
// Estimated by central differences — samples scene() 6 times near the hit.

vec3 calcNormal(vec3 p) {
    float e = 0.001;
    return normalize(vec3(
        scene(p + vec3(e, 0, 0)).x - scene(p - vec3(e, 0, 0)).x,
        scene(p + vec3(0, e, 0)).x - scene(p - vec3(0, e, 0)).x,
        scene(p + vec3(0, 0, e)).x - scene(p - vec3(0, 0, e)).x
    ));
}

// ── Soft shadows ──────────────────────────────────────────────────────────
// Marches a secondary ray toward the light and measures how close
// it gets to occluders — closer = softer shadow.

float softShadow(vec3 ro, vec3 rd, float mint, float maxt, float k) {
    float res = 1.0;
    float t   = mint;
    for (int i = 0; i < 24; i++) {
        float h = scene(ro + rd * t).x;
        if (h < 0.001) return 0.0;
        res = min(res, k * h / t);
        t  += clamp(h, 0.02, 0.2);
        if (t > maxt) break;
    }
    return clamp(res, 0.0, 1.0);
}

// ── Ambient occlusion ─────────────────────────────────────────────────────
// Samples scene() along the normal at increasing distances.
// Surfaces in tight corners occlude faster = darker.

float ambientOcclusion(vec3 p, vec3 n) {
    float occ = 0.0;
    float sca = 1.0;
    for (int i = 0; i < 5; i++) {
        float h   = 0.01 + 0.12 * float(i) / 4.0;
        float d   = scene(p + h * n).x;
        occ      += (h - d) * sca;
        sca      *= 0.95;
    }
    return clamp(1.0 - 3.0 * occ, 0.0, 1.0);
}

// ── Material shading ──────────────────────────────────────────────────────

vec3 material(float mat, vec3 p, vec3 n, vec3 rd, vec3 lightDir, vec3 lightCol) {
    vec3 col;

    if (mat < 1.5) {
        // Floor — checker pattern
        float chk = mod(floor(p.x) + floor(p.z), 2.0);
        col = mix(vec3(0.12, 0.12, 0.15), vec3(0.22, 0.22, 0.28), chk);
    } else if (mat < 2.5) {
        // Central sphere — iridescent surface
        float fresnel = pow(1.0 - abs(dot(n, -rd)), 3.0);
        col = mix(
            vec3(0.08, 0.12, 0.28),
            vec3(0.8, 0.4, 0.1),
            fresnel
        );
        col = mix(col, vec3(0.9, 0.95, 1.0), u_reflectivity * fresnel);
    } else if (mat < 3.5) {
        // Tori — electric blue/violet
        float band = 0.5 + 0.5 * sin(p.y * 8.0 + uTime * 2.0);
        col = mix(vec3(0.05, 0.2, 0.9), vec3(0.6, 0.1, 0.9), band);
    } else {
        // Boxes/octahedra — warm gold
        col = vec3(0.9, 0.6, 0.1);
    }

    return col;
}

// ── Main ──────────────────────────────────────────────────────────────────

void main() {
    vec2  uv  = (vec2(v_uv.x, 1.0 - v_uv.y) * 2.0 - 1.0) * vec2(uResolution.x / uResolution.y, 1.0);

    // ── Camera ────────────────────────────────────────────────────────────
    float angle = u_cam_angle * TAU;
    float elev  = mix(-0.3, 1.2, u_cam_elevation);
    float dist  = mix(2.0, 12.0, u_cam_distance);

    vec3  camPos = vec3(
        cos(angle) * cos(elev),
        sin(elev),
        sin(angle) * cos(elev)
    ) * dist;

    vec3  camTarget = vec3(0.0, 0.0, 0.0);
    vec3  camFwd    = normalize(camTarget - camPos);
    vec3  camRight  = normalize(cross(camFwd, vec3(0.0, 1.0, 0.0)));
    vec3  camUp     = cross(camRight, camFwd);

    // Ray direction — perspective projection
    vec3  rd = normalize(uv.x * camRight + uv.y * camUp + 1.8 * camFwd);
    vec3  ro  = camPos;

    // ── Raymarching ───────────────────────────────────────────────────────
    float tMax = 30.0;
    float t    = 0.0;
    float mat  = -1.0;

    for (int i = 0; i < 128; i++) {
        vec2  res = scene(ro + rd * t);
        float d   = res.x;
        if (d < 0.001) { mat = res.y; break; }
        if (t > tMax)  break;
        t += d;
    }

    // ── Shading ───────────────────────────────────────────────────────────
    vec3 col;

    if (mat > 0.0) {
        vec3 p  = ro + rd * t;
        vec3 n  = calcNormal(p);

        // Light — position and color from MIDI
        vec3  lightPos = vec3(3.0, 5.0, 2.0);
        vec3  lightDir = normalize(lightPos - p);
        float lightTemp = u_light_temp;
        vec3  lightCol = mix(
            vec3(1.0, 0.7, 0.4),   // warm tungsten
            vec3(0.6, 0.8, 1.0),   // cool daylight
            lightTemp
        ) * 2.5;

        // Diffuse
        float diff = max(dot(n, lightDir), 0.0);

        // Specular (Blinn-Phong)
        vec3  h    = normalize(lightDir - rd);
        float spec = pow(max(dot(n, h), 0.0), 64.0) * u_reflectivity;

        // Shadow + AO
        float shadow = softShadow(p + n * 0.01, lightDir, 0.02, 8.0, 12.0);
        float ao     = ambientOcclusion(p, n);

        // Ambient
        vec3  ambient = vec3(0.04, 0.06, 0.1) * ao;

        // Surface color
        vec3  surfCol = material(mat, p, n, rd, lightDir, lightCol);

        col  = surfCol * (ambient + lightCol * diff * shadow);
        col += lightCol * spec * shadow;

        // Subsurface glow on rounded objects
        float sss = pow(max(dot(-rd, n), 0.0), 2.0) * 0.15;
        col += surfCol * sss * mix(vec3(1.0, 0.5, 0.2), vec3(0.3, 0.5, 1.0), lightTemp);

        // Distance fog
        float fogAmount = 1.0 - exp(-t * u_fog * 0.12);
        vec3  fogCol    = mix(vec3(0.02, 0.03, 0.08), vec3(0.08, 0.06, 0.12), lightTemp);
        col = mix(col, fogCol, fogAmount);

    } else {
        // Background — gradient sky dome
        float horizon = smoothstep(-0.1, 0.3, rd.y);
        vec3  skyLow  = mix(vec3(0.04, 0.04, 0.1), vec3(0.12, 0.08, 0.2), u_light_temp);
        vec3  skyHigh = mix(vec3(0.02, 0.04, 0.15), vec3(0.05, 0.1, 0.25), u_light_temp);
        col = mix(skyLow, skyHigh, horizon);

        // Stars — only in clear areas
        float stars = step(0.997, fract(sin(dot(floor(rd * 400.0), vec3(127.1, 311.7, 74.3))) * 43758.5));
        col += stars * (1.0 - u_fog) * 0.6;
    }

    // ── Post-processing ───────────────────────────────────────────────────

    // Tone mapping (ACES approximation)
    col = col * (2.51 * col + 0.03) / (col * (2.43 * col + 0.59) + 0.14);

    // Gamma correction
    col = pow(clamp(col, 0.0, 1.0), vec3(0.4545));

    // Vignette
    float vignette = 1.0 - dot(v_uv - 0.5, (v_uv - 0.5) * 1.8);
    col *= vignette;

    fragColor = vec4(col, 1.0);
}
