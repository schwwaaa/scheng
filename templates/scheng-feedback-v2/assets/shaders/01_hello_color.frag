// ═══════════════════════════════════════════════════════════════════════════
// LESSON 01 — Hello Colour
// Your first fragment shader. Every pixel runs this code independently.
// ═══════════════════════════════════════════════════════════════════════════
//
// What you have available (injected automatically by scheng):
//
//   v_uv         — UV coordinates. (0,0)=bottom-left, (1,1)=top-right.
//   uTime        — seconds elapsed since the instrument started.
//   uResolution  — vec2(width, height) in pixels.
//   fragColor    — write your output colour here as vec4(R, G, B, Alpha).
//
// RGB values range 0.0–1.0:
//   vec3(1, 0, 0) = red
//   vec3(0, 1, 0) = green
//   vec3(0, 0, 1) = blue
//   vec3(1, 1, 1) = white
//   vec3(0, 0, 0) = black
//
// ── MIDI controls ─────────────────────────────────────────────────────────
// Move CC1–CC8 on your MIDI controller to see what they do.

uniform float u_p1;  // CC1 — red channel
uniform float u_p2;  // CC2 — green channel
uniform float u_p3;  // CC3 — blue channel
uniform float u_p4;  // CC4 — animation speed
uniform float u_p5;  // CC5 — pulse depth
uniform float u_p6;  // CC6 — unused (try adding something!)
uniform float u_p7;  // CC7 — unused
uniform float u_p8;  // CC8 — unused

void main() {
    // v_uv.x goes from 0 (left) to 1 (right)
    // v_uv.y goes from 0 (bottom) to 1 (top)

    // Animate each channel with a sine wave
    float t = uTime * (0.3 + u_p4 * 1.0);

    float r = u_p1 * (0.5 + 0.5 * sin(t));
    float g = u_p2 * (0.5 + 0.5 * sin(t + 2.094));   // 2π/3 phase offset
    float b = u_p3 * (0.5 + 0.5 * sin(t + 4.189));   // 4π/3 phase offset

    // Add a brightness pulse from CC5
    float pulse = 1.0 + u_p5 * 0.5 * sin(t * 3.0);

    vec3 col = vec3(r, g, b) * pulse;

    // Always output alpha = 1.0 (fully opaque)
    fragColor = vec4(col, 1.0);

    // ── TRY THESE MODIFICATIONS ─────────────────────────────────────────
    // Make colour change with X position:
    //   fragColor = vec4(v_uv.x, v_uv.y, 0.5, 1.0);
    //
    // Make a horizontal gradient:
    //   fragColor = vec4(vec3(v_uv.x), 1.0);
    //
    // Make colour change with time only:
    //   fragColor = vec4(0.5 + 0.5*sin(uTime), 0.0, 0.0, 1.0);
}
