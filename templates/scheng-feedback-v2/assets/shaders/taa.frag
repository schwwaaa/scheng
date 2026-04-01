// taa.frag — Temporal Anti-Aliasing via PreviousFrame
//
// iChannel0 = previous composited frame (PreviousFrame node)
//
// How it works:
//   Each frame, render the scene at a sub-pixel jittered UV offset.
//   Blend the jittered current frame with the previous frame.
//   Over 8 frames, 8 different sub-pixel positions are covered,
//   producing effectively 8× supersampled quality for free.
//
// The jitter pattern is a Halton(2,3) sequence — well-distributed,
// no perceptible pattern, converges to clean AA after ~8 frames.
//
// Quality: considerably better than MSAA 4× for shader content.
// Cost: ~same as rendering without AA (one texture sample per pixel overhead).
//
// CC1 = scene speed       CC5 = TAA blend weight (0=more TAA, 1=less/sharper)
// CC2 = scene scale       CC6 = jitter radius (0.5=1px, higher=smoother)
// CC3 = scene complexity  CC7 = scene colour
// CC4 = hue               CC8 = scene animation style

uniform float u_p1;
uniform float u_p2;
uniform float u_p3;
uniform float u_p4;
uniform float u_p5;
uniform float u_p6;
uniform float u_p7;
uniform float u_p8;

// ── Halton low-discrepancy sequence ──────────────────────────────────────
// Returns the n-th term of Halton sequence in given base.
// Covers [0,1) more evenly than random, converges faster for AA.
float halton(int index, int base) {
    float f = 1.0, r = 0.0;
    int i = index;
    for (int j = 0; j < 8; j++) {
        if (i <= 0) break;
        f = f / float(base);
        r = r + f * float(i - (i / base) * base);
        i = i / base;
    }
    return r;
}

// ── Scene: SDF content to be anti-aliased ────────────────────────────────
// Using the same SDF geometry as the playground's 05_sdf2d for consistency.
float sdCircle(vec2 p, float r) { return length(p) - r; }
float sdBox(vec2 p, vec2 b) { vec2 d=abs(p)-b; return length(max(d,0.))+min(max(d.x,d.y),0.); }
float sdTri(vec2 p, float r) {
    const float k=1.732; p.x=abs(p.x)-r; p.y=p.y+r/k;
    if(p.x+k*p.y>0.) p=vec2(p.x-k*p.y,-k*p.x-p.y)/2.;
    p.x-=clamp(p.x,-2.*r,0.); return -length(p)*sign(p.y);
}
float smin(float a, float b, float k) {
    float h=clamp(.5+.5*(b-a)/k,0.,1.); return mix(b,a,h)-k*h*(1.-h);
}

vec3 renderScene(vec2 uv) {
    float t   = uTime * (0.2 + u_p1 * 0.8);
    float sz  = 0.12 + u_p2 * 0.12;
    float orb = 0.25 + u_p2 * 0.25;
    int   N   = 1 + int(u_p3 * 4.0);
    float k   = 0.02 + u_p3 * 0.1;
    float px  = 2.0 / min(uResolution.x, uResolution.y);

    float d = 1e9;
    for (int i = 0; i < 5; i++) {
        if (i >= N) break;
        float a = t + float(i) * 6.28318 / float(N);
        vec2 c0 = vec2(cos(a), sin(a)) * orb;
        vec2 c1 = vec2(cos(a+2.094), sin(a+2.094)) * orb;
        vec2 c2 = vec2(cos(a+4.189), sin(a+4.189)) * orb;
        d = smin(d, sdCircle(uv - c0, sz),              k);
        d = smin(d, sdBox   (uv - c1, vec2(sz * 0.85)), k);
        d = smin(d, sdTri   (uv - c2, sz * 1.1),        k);
    }
    d = smin(d, sdCircle(uv, sz * 0.45 + sz * 0.2 * sin(t * 2.1)), 0.03);

    // Analytically smooth edges (used in addition to TAA for double smoothness)
    float fill = 1.0 - smoothstep(-px, px, d);
    float glow = exp(-max(d, 0.0) * 10.0) * 0.4;
    float rim  = 1.0 - smoothstep(0.0, px*3.0, abs(d) - px);

    vec3 fg = 0.55 + 0.45 * cos(u_p4 * 6.28318 + vec3(0.0, 2.1, 4.2));
    vec3 bg = mix(vec3(0.04, 0.04, 0.08), vec3(0.12, 0.06, 0.18), u_p7);

    vec3 col = bg + fg * (fill + glow) + vec3(rim * fill * 0.3);
    return col;
}

void main() {
    // ── Jitter current UV by sub-pixel Halton offset ──────────────────────
    // Frame index loops through 8 Halton positions (2,3 bases)
    int   idx    = int(mod(float(uFrame), 8.0));
    float jx     = halton(idx + 1, 2) - 0.5;  // [-0.5, 0.5] in pixel space
    float jy     = halton(idx + 1, 3) - 0.5;
    float radius = 0.5 + u_p6 * 0.5;           // CC6 scales jitter radius

    // Convert from pixel space to UV space
    vec2 jitter  = vec2(jx, jy) * radius / uResolution;
    vec2 uv_jit  = v_uv + jitter;

    // Aspect-correct centred UV for scene
    vec2 uv_scene = (uv_jit * 2.0 - 1.0);
    uv_scene.x   *= uResolution.x / uResolution.y;

    // ── Render scene at jittered position ────────────────────────────────
    vec3 current = renderScene(uv_scene);

    // ── Sample previous frame ─────────────────────────────────────────────
    // No Y-flip — PreviousFrame stores the render target as-is
    vec3 prev = texture(iChannel0, v_uv).rgb;

    // ── TAA blend ────────────────────────────────────────────────────────
    // CC5=0 → 0.05 weight on current (maximum AA smoothness, slow convergence)
    // CC5=0.5 (default) → 0.1 weight (good balance)
    // CC5=1 → 0.3 weight (less TAA, sharper, faster response to motion)
    float alpha = mix(0.05, 0.30, u_p5);
    vec3  result = mix(prev, current, alpha);

    fragColor = vec4(result, 1.0);
}
