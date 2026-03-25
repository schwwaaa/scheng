// ─────────────────────────────────────────────────────────────────────────────
// scheng shader: ramp-generator.frag
//
// Ramp Generator
// Produces geometric voltage ramps — the foundational waveform of analog
// video synthesis. LZX Visionary, Cadet I (H Ramp), Cadet II (V Ramp).
//
// A "ramp" is a linearly increasing voltage that sweeps across the frame,
// used as a control voltage (CV) input to modulators, oscillators, and
// keyers to create geometric shapes and patterns.
//
// Four ramp types selectable via u_mode:
//   0 = horizontal (left→right)
//   1 = vertical   (top→bottom)
//   2 = radial     (centre→edge, circular)
//   3 = angular    (rotation around centre, 0–1 = 0°–360°)
//
// Node role:  Source  (no input — generates signal)
// Uniforms:
//   u_mode     [0,3]    ramp type                  default: 0
//   u_freq     [0.1,16] tile frequency             default: 1.0
//   u_phase    [0,1]    phase offset               default: 0.0
//   u_invert   [0,1]    invert output              default: 0.0
//   u_center_x [0,1]    centre X for radial/angular default: 0.5
//   u_center_y [0,1]    centre Y for radial/angular default: 0.5
// ─────────────────────────────────────────────────────────────────────────────
#version 330 core
in  vec2 v_uv;
out vec4 fragColor;

uniform float u_mode;
uniform float u_freq;
uniform float u_phase;
uniform float u_invert;
uniform float u_center_x;
uniform float u_center_y;

void main() {
    vec2  uv  = v_uv;
    vec2  ctr = vec2(u_center_x, u_center_y);
    int   mode = int(u_mode);

    float ramp;

    if (mode == 0) {
        // Horizontal: left→right sawtooth
        ramp = fract(uv.x * u_freq + u_phase);

    } else if (mode == 1) {
        // Vertical: top→bottom sawtooth
        ramp = fract(uv.y * u_freq + u_phase);

    } else if (mode == 2) {
        // Radial: distance from centre, tiled
        // Matches LZX Cadet III / Visual Cortex radial output
        float dist = length(uv - ctr) * 2.0; // 0 at centre, 1 at corners
        ramp = fract(dist * u_freq + u_phase);

    } else {
        // Angular: angle around centre, 0→1 = 0°→360°
        // Matches LZX phase/angle modulation inputs
        vec2  d    = uv - ctr;
        float angle = atan(d.y, d.x);              // -π to π
        ramp = fract((angle / (2.0 * 3.14159265)) + 0.5 + u_phase);
    }

    // Optional invert (flip the ramp polarity — common analog patch)
    ramp = mix(ramp, 1.0 - ramp, u_invert);

    fragColor = vec4(ramp, ramp, ramp, 1.0);
}
