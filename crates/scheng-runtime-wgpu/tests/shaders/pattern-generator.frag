// ─────────────────────────────────────────────────────────────────────────────
// scheng shader: pattern-generator.frag
//
// Pattern Generator — SMPTE colour bars, grids, test cards, geometric patterns.
// Const vec3 arrays replaced with functions for naga compatibility.
//
// u_mode: 0=SMPTE75 1=full-bars 2=grid 3=crosshatch 4=testcard 5=circle 6=checker
// ─────────────────────────────────────────────────────────────────────────────
#version 330 core
in  vec2 v_uv;
out vec4 fragColor;

uniform float u_mode;
uniform float u_freq;
uniform float u_line_w;
uniform float u_phase;

// SMPTE 75% colour bars — indexed via if/else (naga doesn't support const vec3 arrays)
vec3 bar75(int i) {
    if      (i == 0) return vec3(0.75, 0.75, 0.75); // White 75%
    else if (i == 1) return vec3(0.75, 0.75, 0.00); // Yellow
    else if (i == 2) return vec3(0.00, 0.75, 0.75); // Cyan
    else if (i == 3) return vec3(0.00, 0.75, 0.00); // Green
    else if (i == 4) return vec3(0.75, 0.00, 0.75); // Magenta
    else if (i == 5) return vec3(0.75, 0.00, 0.00); // Red
    else if (i == 6) return vec3(0.00, 0.00, 0.75); // Blue
    else             return vec3(0.00, 0.00, 0.00); // Black
}

// 100% colour bars
vec3 bar100(int i) {
    if      (i == 0) return vec3(1.00, 1.00, 1.00);
    else if (i == 1) return vec3(1.00, 1.00, 0.00);
    else if (i == 2) return vec3(0.00, 1.00, 1.00);
    else if (i == 3) return vec3(0.00, 1.00, 0.00);
    else if (i == 4) return vec3(1.00, 0.00, 1.00);
    else if (i == 5) return vec3(1.00, 0.00, 0.00);
    else if (i == 6) return vec3(0.00, 0.00, 1.00);
    else             return vec3(0.00, 0.00, 0.00);
}

void main() {
    vec2  uv   = v_uv;
    int   mode = int(u_mode);
    vec3  col  = vec3(0.0);

    if (mode == 0) {
        // SMPTE 75% colour bars
        int bar_idx = clamp(int(uv.x * 8.0), 0, 7);
        col = bar75(bar_idx);

    } else if (mode == 1) {
        // 100% colour bars
        int bar_idx = clamp(int(uv.x * 8.0), 0, 7);
        col = bar100(bar_idx);

    } else if (mode == 2) {
        // Grid: H and V lines
        vec2 grid  = fract(uv * u_freq + u_phase);
        float lw   = u_line_w * u_freq;
        float line_h = step(1.0 - lw, grid.x);
        float line_v = step(1.0 - lw, grid.y);
        col = vec3(max(line_h, line_v));

    } else if (mode == 3) {
        // Crosshatch: diagonal lines
        float d1 = fract((uv.x + uv.y) * u_freq * 0.5 + u_phase);
        float d2 = fract((uv.x - uv.y) * u_freq * 0.5 + u_phase);
        float lw = u_line_w * u_freq * 0.5;
        float l1 = step(1.0 - lw, d1);
        float l2 = step(1.0 - lw, d2);
        col = vec3(max(l1, l2));

    } else if (mode == 4) {
        // Test card: greyscale ramp top, hue ramp bottom
        if (uv.y < 0.5) {
            float luma = fract(uv.x * u_freq + u_phase);
            col = vec3(luma);
        } else {
            float h = fract(uv.x + u_phase);
            float r = clamp(abs(mod(h * 6.0 - 3.0, 6.0) - 3.0) - 1.0, 0.0, 1.0);
            float g = clamp(2.0 - abs(mod(h * 6.0 - 2.0, 6.0) - 2.0), 0.0, 1.0);
            float b = clamp(2.0 - abs(mod(h * 6.0 - 4.0, 6.0) - 2.0), 0.0, 1.0);
            col = vec3(r, g, b);
        }

    } else if (mode == 5) {
        // Circle / disc
        vec2  d    = uv - vec2(0.5);
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
