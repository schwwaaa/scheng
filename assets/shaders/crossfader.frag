// ─────────────────────────────────────────────────────────────────────────────
// scheng shader: crossfader.frag
//
// T-Bar Crossfader
// Blends between two input signals with multiple transition modes.
// Reference: LZX Cadet VIII / video mixer T-bar faders.
//
// Mode 0 = dissolve (linear mix — the broadcast standard T-bar)
// Mode 1 = additive (A + B, clipped — for layering/accretion)
// Mode 2 = multiply (A × B — for texture mapping / gating)
// Mode 3 = hard wipe (left→right at u_tbar position, no softness)
// Mode 4 = soft wipe (left→right with feathered edge)
//
// Node role:  Mixer  (iChannel0=A, iChannel1=B)
// Uniforms:
//   u_tbar    [0, 1]   crossfade position (0=full A, 1=full B)  default: 0.5
//   u_mode    [0, 4]   transition type                          default: 0
//   u_softness [0,0.5] wipe edge softness (modes 3–4)           default: 0.05
// ─────────────────────────────────────────────────────────────────────────────
#version 330 core
in  vec2 v_uv;
out vec4 fragColor;

uniform sampler2D iChannel0;  // input A
uniform sampler2D iChannel1;  // input B
uniform float u_tbar;
uniform float u_mode;
uniform float u_softness;

void main() {
    vec4 a = texture(iChannel0, v_uv);
    vec4 b = texture(iChannel1, v_uv);
    int  mode = int(u_mode);

    vec4 out_col;

    if (mode == 0) {
        // Dissolve — standard linear crossfade
        out_col = mix(a, b, u_tbar);

    } else if (mode == 1) {
        // Additive — both signals add, clipped to 1.0
        out_col = clamp(a * (1.0 - u_tbar) + b * u_tbar + a * b * u_tbar, 0.0, 1.0);

    } else if (mode == 2) {
        // Multiply — A gates B (useful for keying shapes onto sources)
        out_col = mix(a, a * b, u_tbar);

    } else if (mode == 3) {
        // Hard wipe: left portion shows A, right shows B
        // u_tbar = wipe position (0=all A, 1=all B)
        float edge = step(u_tbar, v_uv.x);
        out_col = mix(a, b, edge);

    } else {
        // Soft wipe: feathered transition
        float soft = max(u_softness, 0.001);
        float edge = smoothstep(u_tbar - soft, u_tbar + soft, v_uv.x);
        out_col = mix(a, b, edge);
    }

    fragColor = out_col;
}
