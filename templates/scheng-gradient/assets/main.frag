// assets/shaders/main.frag
// ─────────────────────────────────────────────────────────────────────────────
// scheng-gradient — default shader
//
// This file is hot-reloaded. Save it and the instrument updates immediately.
//
// Available uniforms (injected automatically by scheng):
//   uTime      — seconds since start (float)
//   uFrame     — frame counter (float)
//   uResolution — vec2(width, height)
//
// Available inputs (when I/O features are enabled):
//   iChannel0  — first input texture (Syphon, webcam, video, NDI)
//   iChannel1  — second input texture
//   iChannel2  — third input texture
//   iChannel3  — fourth input texture
//
// Custom uniforms:
//   Any float uniform starting with u_ is automatically exposed via OSC/MIDI.
//   Example: uniform float u_speed; → /scheng/node0/u_speed

void main() {
    vec2 uv = v_uv;

    // Animated gradient — demonstrates uTime and spatial coordinates
    float t = uTime * 0.3;

    vec3 col = 0.5 + 0.5 * cos(t + uv.xyx + vec3(0.0, 2.1, 4.2));

    // Soft vignette
    float vign = 1.0 - smoothstep(0.4, 0.9, length(uv - 0.5) * 1.4);
    col *= vign;

    fragColor = vec4(col, 1.0);
}
