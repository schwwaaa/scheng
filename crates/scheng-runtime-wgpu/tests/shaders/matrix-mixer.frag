// ─────────────────────────────────────────────────────────────────────────────
// scheng shader: matrix-mixer.frag
//
// Matrix Mixer (4→1)
// Weighted sum of up to 4 input channels with individual gain controls.
// Reference: LZX Matrix Mixer, video mixing consoles, Fairlight CMI fader banks.
//
// This is the core routing primitive of analog video synthesis — every
// signal can feed into every output channel with variable gain. Full matrix
// mixing enables any signal topology: summation, inversion, polarisation.
//
// Node role:  Mixer  (iChannel0–3)
// Uniforms:
//   u_gain0  [-2, 2]  gain for iChannel0    default: 1.0
//   u_gain1  [-2, 2]  gain for iChannel1    default: 0.0
//   u_gain2  [-2, 2]  gain for iChannel2    default: 0.0
//   u_gain3  [-2, 2]  gain for iChannel3    default: 0.0
//   u_offset [0, 1]   DC offset (bias)      default: 0.0
//   u_clip   [0, 1]   enable output clipping default: 1.0
// ─────────────────────────────────────────────────────────────────────────────
#version 330 core
in  vec2 v_uv;
out vec4 fragColor;

uniform sampler2D iChannel0;
uniform sampler2D iChannel1;
uniform sampler2D iChannel2;
uniform sampler2D iChannel3;
uniform float u_gain0;
uniform float u_gain1;
uniform float u_gain2;
uniform float u_gain3;
uniform float u_offset;
uniform float u_clip;

void main() {
    vec4 c0 = texture(iChannel0, v_uv);
    vec4 c1 = texture(iChannel1, v_uv);
    vec4 c2 = texture(iChannel2, v_uv);
    vec4 c3 = texture(iChannel3, v_uv);

    // Weighted sum — negative gains invert the signal (phase reversal)
    vec3 mix_out = c0.rgb * u_gain0
                 + c1.rgb * u_gain1
                 + c2.rgb * u_gain2
                 + c3.rgb * u_gain3
                 + vec3(u_offset);

    // Optional hard clip (disable for "hot" overload / bloom effect)
    if (u_clip > 0.5) {
        mix_out = clamp(mix_out, 0.0, 1.0);
    }

    // Alpha: take from channel 0 (primary source)
    fragColor = vec4(mix_out, c0.a);
}
