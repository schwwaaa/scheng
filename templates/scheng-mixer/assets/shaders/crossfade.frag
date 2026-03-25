// crossfade.frag — linear A/B T-bar mix
// iChannel0 = source A
// iChannel1 = source B  
// u_tbar    = 0.0 → full A, 1.0 → full B

uniform float u_tbar;

void main() {
    vec4 a = texture(iChannel0, v_uv);
    vec4 b = texture(iChannel1, v_uv);
    fragColor = mix(a, b, clamp(u_tbar, 0.0, 1.0));
}
