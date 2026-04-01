// ═══════════════════════════════════════════════════════════════════════════
// LESSON 02 — Coordinate Systems
// Learning to work with UV space and how to draw a circle.
// ═══════════════════════════════════════════════════════════════════════════
//
// v_uv is in [0,1]×[0,1]. But screens are wider than tall.
// To draw a circle that isn't an oval, we need aspect-corrected coordinates.
//
// STANDARD PATTERN for centred work:
//   vec2 uv = v_uv * 2.0 - 1.0;           // remap to [-1,1]×[-1,1]
//   uv.x   *= uResolution.x/uResolution.y;  // stretch X to match screen ratio
//
// Now: uv=(0,0) is the screen centre. Circle at radius 0.5 is round.
//
// SIGNED DISTANCE FIELD (SDF):
//   d = length(uv - centre) - radius
//   d < 0 → inside the circle
//   d = 0 → on the edge
//   d > 0 → outside
//
// ─────────────────────────────────────────────────────────────────────────
// CC1 = X position    CC2 = Y position    CC3 = radius
// CC4 = edge softness CC5 = ring mode     CC6 = hue
// CC7 = bg colour     CC8 = animate

uniform float u_p1;
uniform float u_p2;
uniform float u_p3;
uniform float u_p4;
uniform float u_p5;
uniform float u_p6;
uniform float u_p7;
uniform float u_p8;

void main() {
    // Centred, aspect-corrected UV
    vec2 uv = (v_uv * 2.0 - 1.0);
    uv.x   *= uResolution.x / uResolution.y;

    // Circle position from CC1 (X) and CC2 (Y)
    float t   = uTime * u_p8 * 0.5;
    float cx  = (u_p1 - 0.5) * 2.0 * (uResolution.x / uResolution.y);
    float cy  = (u_p2 - 0.5) * 2.0;
    vec2  ctr = vec2(cx + 0.3 * sin(t), cy + 0.2 * cos(t * 1.3));

    float radius = 0.05 + u_p3 * 0.55;  // CC3 controls size

    // Distance from this pixel to the circle edge
    float d = length(uv - ctr) - radius;

    // CC5 switches between filled circle and ring
    float ring_w = 0.03;
    d = mix(d, abs(d) - ring_w, u_p5);

    // Soft edge — smoothstep gives anti-aliased fill
    // Without this: hard pixellated edge. With it: smooth sub-pixel AA.
    float softness = 0.002 + u_p4 * 0.08;
    float fill     = 1.0 - smoothstep(-softness, softness, d);

    // Glow: exponential falloff from edge
    float glow = exp(-max(d, 0.0) * 8.0) * 0.5;

    vec3 fg = 0.5 + 0.5 * cos(u_p6 * 6.28318 + vec3(0, 2.1, 4.2));
    vec3 bg = vec3(u_p7 * 0.15);

    vec3 col = bg + fg * (fill + glow);
    fragColor = vec4(col, 1.0);
}
