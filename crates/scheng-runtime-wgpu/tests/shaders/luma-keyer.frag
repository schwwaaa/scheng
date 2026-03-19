// ─────────────────────────────────────────────────────────────────────────────
// scheng shader: luma-keyer.frag
//
// Luma Keyer
// Generates a key signal (matte) from the luminance of iChannel0, then
// composites foreground (iChannel1) over background (iChannel2).
//
// Reference: LZX Cadet IV (Key Generator), broadcast luma keyers.
// In analog hardware, the key signal is a derived voltage that gates the
// downstream mix — above the threshold lets the foreground through, below
// passes the background. Softness controls the edge transition width.
//
// Node role:  Processor / Mixer  (3 inputs)
//   iChannel0 = key source (luma extracted from this)
//   iChannel1 = foreground (shown where key is HIGH)
//   iChannel2 = background (shown where key is LOW)
//
// Uniforms:
//   u_thresh   [0, 1]   key clip point            default: 0.5
//   u_softness [0, 0.5] edge softness (feather)   default: 0.05
//   u_gain     [0, 4]   pre-key luma amplifier     default: 1.0
//   u_invert   [0, 1]   invert the key matte       default: 0.0
// ─────────────────────────────────────────────────────────────────────────────
#version 330 core
in  vec2 v_uv;
out vec4 fragColor;

uniform sampler2D iChannel0;  // key source
uniform sampler2D iChannel1;  // foreground
uniform sampler2D iChannel2;  // background
uniform float u_thresh;
uniform float u_softness;
uniform float u_gain;
uniform float u_invert;

void main() {
    vec4 key_src = texture(iChannel0, v_uv);
    vec4 fg      = texture(iChannel1, v_uv);
    vec4 bg      = texture(iChannel2, v_uv);

    // Extract luma (Rec.709)
    float luma = dot(key_src.rgb, vec3(0.2126, 0.7152, 0.0722));

    // Apply pre-key gain (amplify signal before clipping — analog hardware behaviour)
    luma = clamp(luma * u_gain, 0.0, 1.0);

    // Soft key: smoothstep over [thresh - softness, thresh + softness]
    // Hard key when softness = 0 (step function, matching a comparator circuit)
    float soft   = max(u_softness, 0.0001); // prevent div-by-zero
    float key    = smoothstep(u_thresh - soft, u_thresh + soft, luma);

    // Invert the key matte (flip foreground/background)
    key = mix(key, 1.0 - key, u_invert);

    // Composite: fg where key=1, bg where key=0
    fragColor = mix(bg, fg, key);
}
