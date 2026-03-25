// solarize.frag — classic Sabattier/solarize effect
// Inverts pixels above a threshold — immediately obvious on any input

uniform float u_threshold; // 0.0-1.0, default 0.5 (MIDI CC1)
uniform float u_mix;       // 0.0-1.0 wet/dry mix, default 1.0 (MIDI CC2)

void main() {
    vec4 src = texture(iChannel0, v_uv);
    vec3 col = src.rgb;

    // Solarize: invert values above threshold
    vec3 solar = mix(col, 1.0 - col, step(u_threshold, col));

    // Wet/dry
    fragColor = vec4(mix(col, solar, u_mix), 1.0);
}
