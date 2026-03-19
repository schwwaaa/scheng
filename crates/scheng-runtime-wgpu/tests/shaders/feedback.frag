// ─────────────────────────────────────────────────────────────────────────────
// scheng shader: feedback.frag
//
// Video Feedback
// Mixes the current frame with the previous frame at reduced gain, creating
// temporal echo / feedback trails. The foundational texture of analog video
// feedback systems (pointing a camera at a monitor, simulated digitally).
//
// Reference: LZX Cadet VI (Lag Processor), video feedback installations,
// Steina & Woody Vasulka feedback works.
//
// In hardware this is achieved by mixing the live signal with a delayed
// version (via a frame buffer or physical camera-monitor loop) at a gain
// less than unity so the feedback converges rather than exploding.
//
// Node role:  Processor  (iChannel0=live, iChannel1=previous frame)
// Uniforms:
//   u_decay      [0, 0.99]  feedback decay (0=no feedback, 0.99=long tail)
//   u_zoom       [0.9, 1.1] zoom factor applied to previous frame
//   u_rotation   [-5, 5]    rotation in degrees per frame
//   u_offset_x   [-0.1,0.1] horizontal drift per frame
//   u_offset_y   [-0.1,0.1] vertical drift per frame
//   u_blend_mode [0, 1]     0=additive, 1=mix
//
// Note: iChannel1 must be connected to the PreviousFrame node in the graph
//       for this shader to create actual feedback loops.
// ─────────────────────────────────────────────────────────────────────────────
#version 330 core
in  vec2 v_uv;
out vec4 fragColor;

uniform sampler2D iChannel0;  // live source
uniform sampler2D iChannel1;  // previous frame (connect PreviousFrame node here)
uniform float u_decay;
uniform float u_zoom;
uniform float u_rotation;
uniform float u_offset_x;
uniform float u_offset_y;
uniform float u_blend_mode;

void main() {
    vec2 uv = v_uv;

    // Transform UV for the previous frame sample —
    // zoom, rotate, and offset create the characteristic feedback spiral
    vec2 ctr  = vec2(0.5, 0.5);
    vec2 d    = uv - ctr;

    // Rotation
    float angle = radians(u_rotation);
    float cosA  = cos(angle);
    float sinA  = sin(angle);
    d = vec2(d.x * cosA - d.y * sinA, d.x * sinA + d.y * cosA);

    // Zoom
    d /= max(u_zoom, 0.001);

    // Drift offset
    vec2 fb_uv = ctr + d + vec2(u_offset_x, u_offset_y);

    vec4 live     = texture(iChannel0, uv);
    vec4 previous = texture(iChannel1, fb_uv);

    // Decay: previous frame contributes at reduced amplitude
    vec4 decayed  = previous * u_decay;

    // Blend modes
    vec4 result;
    if (u_blend_mode < 0.5) {
        // Additive: live + decayed previous (creates bloom / trails)
        result = clamp(live + decayed, 0.0, 1.0);
    } else {
        // Mix: crossfade between live and decayed previous
        result = mix(live, decayed, u_decay);
    }

    fragColor = result;
}
