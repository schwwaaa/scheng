// crossfade.frag — linear T-bar mix between two video sources
// iChannel0 = video A
// iChannel1 = video B
// u_tbar    = 0.0 → full A, 1.0 → full B  (MIDI CC1)

uniform float u_tbar;

void main() {
    vec4 a = texture(iChannel0, v_uv);
    vec4 b = texture(iChannel1, v_uv);
    fragColor = mix(a, b, clamp(u_tbar, 0.0, 1.0));
}
