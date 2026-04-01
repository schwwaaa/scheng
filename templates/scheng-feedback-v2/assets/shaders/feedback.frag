// feedback.frag — real temporal feedback via PreviousFrame
//
// iChannel0 = previous rendered frame (PreviousFrame node, no Y-flip needed)
//
// Signal chain each frame:
//   1. Sample previous frame, apply spatial transform (zoom + rotation)
//   2. Decay — multiply toward black
//   3. Hue drift — trails slowly shift colour
//   4. Generate SPARSE foreground: SDF shapes on PURE BLACK
//      ↑ This is the critical requirement. Background = vec3(0.0). Always.
//      Non-black background drowns the feedback buffer.
//   5. ADDITIVE composite — shapes accumulate on buffer, never replace it
//   6. Reinhard tone map — prevents overflow as trails stack up
//
// CC1 = orbit speed       CC5 = decay (0=fast fade, 1=long trails)
// CC2 = shape size        CC6 = zoom  (0.5=none, <0.5=expand, >0.5=contract)
// CC3 = shape count       CC7 = rotation per frame (0.5=none, away=CW/CCW)
// CC8 = hue drift         CC4 = shape hue

uniform float u_p1;  // CC1 orbit speed
uniform float u_p2;  // CC2 shape size
uniform float u_p3;  // CC3 shape count
uniform float u_p4;  // CC4 shape hue
uniform float u_p5;  // CC5 decay
uniform float u_p6;  // CC6 zoom
uniform float u_p7;  // CC7 rotation
uniform float u_p8;  // CC8 hue drift

// ── Hue rotation (correct 3×3 matrix) ────────────────────────────────────
vec3 rotateHue(vec3 rgb, float angle) {
    float c = cos(angle), s = sin(angle);
    mat3 m = mat3(
        0.213+c*0.787-s*0.213,  0.213-c*0.213+s*0.143,  0.213-c*0.213-s*0.787,
        0.715-c*0.715-s*0.715,  0.715+c*0.285+s*0.140,  0.715-c*0.715+s*0.715,
        0.072-c*0.072+s*0.928,  0.072-c*0.072-s*0.283,  0.072+c*0.928+s*0.072
    );
    return clamp(m * rgb, 0.0, 1.0);
}

// ── SDF primitives ────────────────────────────────────────────────────────
float sdCircle(vec2 p, float r) { return length(p) - r; }
float sdRing(vec2 p, float r, float w) { return abs(length(p) - r) - w; }
float sdBox(vec2 p, vec2 b) {
    vec2 d = abs(p) - b; return length(max(d,0.0)) + min(max(d.x,d.y),0.0);
}
float smin(float a, float b, float k) {
    float h = clamp(0.5+0.5*(b-a)/k, 0.0, 1.0);
    return mix(b,a,h) - k*h*(1.0-h);
}

void main() {
    // UV for foreground generation — aspect-corrected, centred
    vec2 uv = (v_uv * 2.0 - 1.0);
    uv.x   *= uResolution.x / uResolution.y;

    // Pixel size for smooth SDF edges
    float px = 2.0 / min(uResolution.x, uResolution.y);

    // ── 1. Sample + spatially transform previous frame ────────────────────
    // No Y-flip here — PreviousFrame stores the render target as-is.
    // A pixel written at v_uv=(x,y) is read back at (x,y) next frame.
    vec2 uvc = v_uv - 0.5;

    // Zoom: 0.5=none, >0.5=contract (trails spiral in), <0.5=expand (trails bloom out)
    float zoom = 1.0 + (u_p6 - 0.5) * 0.012;
    uvc /= zoom;

    // Rotation: 0.5=none, away from 0.5 = CW/CCW spiral trails
    float rot = (u_p7 - 0.5) * 0.014;
    float cr = cos(rot), sr = sin(rot);
    uvc = vec2(cr*uvc.x - sr*uvc.y, sr*uvc.x + cr*uvc.y);

    vec2 fb_uv = uvc + 0.5;

    vec3 prev = vec3(0.0);
    if (fb_uv.x > 0.001 && fb_uv.x < 0.999 &&
        fb_uv.y > 0.001 && fb_uv.y < 0.999) {
        prev = texture(iChannel0, fb_uv).rgb;
    }

    // ── 2. Decay ──────────────────────────────────────────────────────────
    // CC5=0 → 0.85 (very short trails)
    // CC5=0.75 (default) → ~0.975 (medium-long trails)
    // CC5=1 → 0.995 (very long trails)
    float decay = mix(0.85, 0.995, u_p5);
    prev *= decay;

    // ── 3. Hue drift on trails ────────────────────────────────────────────
    // CC8=0.5 → no drift. Away from 0.5 → trails rainbow-shift over time.
    float drift = (u_p8 - 0.5) * 0.05;
    if (abs(drift) > 0.0005) {
        prev = rotateHue(prev, drift);
    }

    // ── 4. Generate foreground — SPARSE shapes on PURE BLACK ─────────────
    // THE RULE: fg background must be vec3(0.0).
    // Any non-black background floods the buffer and kills trails.
    float t   = uTime * (0.2 + u_p1 * 1.2);
    float sz  = 0.04 + u_p2 * 0.12;
    float orb = 0.25 + u_p2 * 0.3;
    int   N   = 1 + int(u_p3 * 4.0);  // 1–5 shapes

    float d = 1e9;

    // Orbiting shapes — alternate circle / ring / box
    for (int i = 0; i < 5; i++) {
        if (i >= N) break;
        float a = t * (0.8 + float(i) * 0.07)
                + float(i) * 6.28318 / float(N);
        vec2 centre = vec2(cos(a), sin(a)) * orb;

        float shape;
        int si = int(mod(float(i), 3.0));
        if      (si == 0) shape = sdCircle(uv - centre, sz);
        else if (si == 1) shape = sdRing  (uv - centre, sz * 0.9, sz * 0.18);
        else              shape = sdBox   (uv - centre, vec2(sz * 0.8));

        d = smin(d, shape, 0.02 + u_p3 * 0.03);
    }

    // Centre pulse — always present as anchor
    float pulse = sdCircle(uv, sz * 0.55 + sz * 0.25 * sin(t * 2.3));
    d = smin(d, pulse, 0.02);

    // Smooth AA fill + glow
    float fill = 1.0 - smoothstep(-px, px, d);
    float glow = exp(-max(d, 0.0) * 10.0) * 0.6;

    // Shape colour — bright, saturated
    vec3 shape_hue = 0.6 + 0.4 * cos(u_p4 * 6.28318 + vec3(0.0, 2.094, 4.189));

    // Foreground: shapes + glow on BLACK (vec3(0))
    // No background, no sky, no atmosphere — only the shapes themselves.
    vec3 fg = shape_hue * (fill + glow);

    // ── 5. Additive composite ─────────────────────────────────────────────
    // fg is black everywhere except at shapes.
    // Adding fg to prev means shapes accumulate — previous frame is preserved.
    // This is fundamentally different from mix() / luma key which replaces.
    vec3 result = prev + fg;

    // ── 6. Reinhard tone map — prevents infinite accumulation / clipping ──
    // Without this, repeated addition saturates to white immediately.
    result = result / (1.0 + result);

    // Mild vignette — keeps bright trails from wrapping off edges
    float vig = 1.0 - smoothstep(0.35, 0.75, length(v_uv - 0.5) * 1.8);
    result *= vig;

    fragColor = vec4(result, 1.0);
}
