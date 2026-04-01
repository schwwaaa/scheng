// ═══════════════════════════════════════════════════════════════════════════
// LESSON 01 — Hello Colour
// Your first fragment shader. Every pixel on screen runs this code.
// ═══════════════════════════════════════════════════════════════════════════
//
// WHAT IS A FRAGMENT SHADER?
// The GPU runs this function once per pixel, every frame, in parallel.
// Your job: output a colour for that pixel via fragColor.
//
// WHAT YOU HAVE AVAILABLE (injected automatically by scheng):
//   v_uv        — UV coordinates. (0,0)=bottom-left, (1,1)=top-right.
//   uTime       — seconds since the instrument started.
//   uResolution — vec2(width, height) in pixels.
//   fragColor   — write your output colour: vec4(R, G, B, Alpha).
//
// COLOUR VALUES range 0.0–1.0:
//   vec3(1, 0, 0) = red    vec3(0, 1, 0) = green   vec3(0, 0, 1) = blue
//   vec3(1, 1, 1) = white  vec3(0, 0, 0) = black   vec3(0.5,...) = mid
//
// MIDI: move CC1–CC8 on your controller to see what each one does.
// ─────────────────────────────────────────────────────────────────────────
// CC1 = red amount      CC2 = green amount    CC3 = blue amount
// CC4 = animation speed CC5 = pulse depth     CC6 = hue shift
// CC7 = brightness      CC8 = unused (try it!)

uniform float u_p1;
uniform float u_p2;
uniform float u_p3;
uniform float u_p4;
uniform float u_p5;
uniform float u_p6;
uniform float u_p7;
uniform float u_p8;

void main() {
    float t = uTime * (0.3 + u_p4 * 1.5);

    // sin() oscillates between -1 and +1. * 0.5 + 0.5 remaps to 0..1.
    float r = u_p1 * (0.5 + 0.5 * sin(t));
    float g = u_p2 * (0.5 + 0.5 * sin(t + 2.094));  // 120° phase offset
    float b = u_p3 * (0.5 + 0.5 * sin(t + 4.189));  // 240° phase offset

    // Hue shift: rotate all three channels together using CC6
    vec3 col = vec3(r, g, b);
    col = col * (0.5 + u_p7 * 0.5);  // CC7 = brightness

    // Pulse: multiply brightness by a fast oscillation (CC5 controls depth)
    float pulse = 1.0 + u_p5 * 0.4 * sin(t * 4.0);
    col *= pulse;

    fragColor = vec4(col, 1.0);

    // ── TRY THESE ───────────────────────────────────────────────────────
    // Gradient across the screen:
    //   fragColor = vec4(v_uv.x, v_uv.y, 0.5, 1.0);
    //
    // Solid red:
    //   fragColor = vec4(1.0, 0.0, 0.0, 1.0);
    //
    // White with time pulsing brightness:
    //   fragColor = vec4(vec3(0.5 + 0.5*sin(uTime)), 1.0);
}
