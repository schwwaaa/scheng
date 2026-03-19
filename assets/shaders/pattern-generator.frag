// ─────────────────────────────────────────────────────────────────────────────
// scheng shader: pattern-generator.frag
//
// Pattern Generator
// Produces standard test patterns and geometric signal sources.
// Reference: SMPTE EG 1 (colour bars), EIA RS-189 (colour bars),
// LZX Visual Cortex sync/pattern outputs, Tektronix 1910 signal generator.
//
// Mode 0 = SMPTE 75% colour bars (broadcast alignment standard)
// Mode 1 = Full-field colour bars (100% amplitude)
// Mode 2 = Grid (H + V lines, configurable frequency)
// Mode 3 = Crosshatch (diagonal grid)
// Mode 4 = Ramp + colour field test card
// Mode 5 = Circle / disc generator
// Mode 6 = Checkerboard
//
// Node role:  Source  (no input)
// Uniforms:
//   u_mode    [0, 6]   pattern type            default: 0
//   u_freq    [1, 32]  grid/pattern frequency   default: 8.0
//   u_line_w  [0, 0.1] line width               default: 0.02
//   u_phase   [0, 1]   pattern phase offset     default: 0.0
// ─────────────────────────────────────────────────────────────────────────────
#version 330 core
in  vec2 v_uv;
out vec4 fragColor;

uniform float u_mode;
uniform float u_freq;
uniform float u_line_w;
uniform float u_phase;

// Standard SMPTE 75% colour bar colours (normalised to 0-1)
const vec3 BARS_75[8] = vec3[8](
    vec3(0.75, 0.75, 0.75),  // White 75%
    vec3(0.75, 0.75, 0.00),  // Yellow
    vec3(0.00, 0.75, 0.75),  // Cyan
    vec3(0.00, 0.75, 0.00),  // Green
    vec3(0.75, 0.00, 0.75),  // Magenta
    vec3(0.75, 0.00, 0.00),  // Red
    vec3(0.00, 0.00, 0.75),  // Blue
    vec3(0.00, 0.00, 0.00)   // Black
);

// Standard 100% colour bar colours
const vec3 BARS_100[8] = vec3[8](
    vec3(1.00, 1.00, 1.00),  // White
    vec3(1.00, 1.00, 0.00),  // Yellow
    vec3(0.00, 1.00, 1.00),  // Cyan
    vec3(0.00, 1.00, 0.00),  // Green
    vec3(1.00, 0.00, 1.00),  // Magenta
    vec3(1.00, 0.00, 0.00),  // Red
    vec3(0.00, 0.00, 1.00),  // Blue
    vec3(0.00, 0.00, 0.00)   // Black
);

void main() {
    vec2  uv   = v_uv;
    int   mode = int(u_mode);
    vec3  col  = vec3(0.0);

    if (mode == 0 || mode == 1) {
        // Colour bars: 8 equal-width vertical bars
        int bar_idx = int(uv.x * 8.0);
        bar_idx = clamp(bar_idx, 0, 7);
        if (mode == 0) col = BARS_75[bar_idx];
        else            col = BARS_100[bar_idx];

    } else if (mode == 2) {
        // Grid: H and V lines at u_freq intervals
        vec2 grid  = fract(uv * u_freq + u_phase);
        float line_h = step(1.0 - u_line_w * u_freq, grid.x);
        float line_v = step(1.0 - u_line_w * u_freq, grid.y);
        col = vec3(max(line_h, line_v));

    } else if (mode == 3) {
        // Crosshatch: diagonal lines at ±45°
        vec2  d1   = fract((uv.x + uv.y) * u_freq * 0.5 + u_phase);
        vec2  d2   = fract((uv.x - uv.y) * u_freq * 0.5 + u_phase);
        float lw   = u_line_w * u_freq * 0.5;
        float l1   = step(1.0 - lw, d1.x);
        float l2   = step(1.0 - lw, d2.x);
        col = vec3(max(l1, l2));

    } else if (mode == 4) {
        // Test card: luma ramp on top half, chroma ramp on bottom half
        if (uv.y < 0.5) {
            // Top: greyscale ramp
            float luma = fract(uv.x * u_freq + u_phase);
            col = vec3(luma);
        } else {
            // Bottom: hue ramp (R→G→B→R)
            float h = fract(uv.x + u_phase);
            // HSV to RGB for a pure hue sweep at full sat/val
            float r = clamp(abs(mod(h * 6.0 - 3.0, 6.0) - 3.0) - 1.0, 0.0, 1.0);
            float g = clamp(2.0 - abs(mod(h * 6.0 - 2.0, 6.0) - 2.0), 0.0, 1.0);
            float b = clamp(2.0 - abs(mod(h * 6.0 - 4.0, 6.0) - 2.0), 0.0, 1.0);
            col = vec3(r, g, b);
        }

    } else if (mode == 5) {
        // Circle / disc: filled circle at centre
        vec2  d    = uv - vec2(0.5 + u_phase, 0.5);
        float r    = length(d);
        float freq_r = 0.5 / max(u_freq * 0.125, 0.01);
        float disc = 1.0 - smoothstep(freq_r - 0.005, freq_r + 0.005, r);
        col = vec3(disc);

    } else {
        // Checkerboard
        vec2  cell = floor(uv * u_freq + u_phase);
        float chk  = mod(cell.x + cell.y, 2.0);
        col = vec3(chk);
    }

    fragColor = vec4(col, 1.0);
}
