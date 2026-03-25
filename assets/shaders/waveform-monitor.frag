// ─────────────────────────────────────────────────────────────────────────────
// scheng shader: waveform-monitor.frag
//
// Waveform Monitor
// Displays the luma or RGB signal level of each column of pixels as a
// waveform trace, exactly as a broadcast waveform monitor does.
//
// Reference: Tektronix 1740 series, Leader LV-5800, broadcast QC practice.
// Used for checking IRE levels, clipping, luma distribution, and legality.
//
// Mode 0 = Luma parade (single trace, grey on black)
// Mode 1 = RGB parade  (R, G, B traces side by side, false colour)
// Mode 2 = Overlay     (all traces overlaid on the input image)
//
// Node role:  Processor  (iChannel0 = signal to analyse)
// Uniforms:
//   u_mode       [0, 2]   display mode               default: 0
//   u_intensity  [0, 1]   trace brightness            default: 0.8
//   u_persistence [0,32]  vertical trace thickness    default: 1.5
// ─────────────────────────────────────────────────────────────────────────────
#version 330 core
in  vec2 v_uv;
out vec4 fragColor;

uniform sampler2D iChannel0;
uniform float u_mode;
uniform float u_intensity;
uniform float u_persistence;
uniform vec2  uResolution;

void main() {
    int  mode = int(u_mode);
    vec2 uv   = v_uv;
    float px_h = 1.0 / uResolution.y; // one pixel in UV space

    // ── Mode 0/1: Parade modes ────────────────────────────────────────────
    // Read the source pixel at (this column, scaled from waveform Y position)
    // and check if the signal level falls at this Y position.

    // Map waveform Y: bottom = 0.0, top = 1.0 (IRE 0 to 100)
    float wave_y = uv.y;  // 0=black(bottom), 1=white(top)

    if (mode == 0) {
        // Luma parade — single column analysis
        // Sample the source at this X column, Y = 50% (representative row)
        // then scatter the luma value vertically
        vec2  src_uv = vec2(uv.x, 0.5);
        vec3  src    = texture(iChannel0, src_uv).rgb;
        float luma   = dot(src, vec3(0.2126, 0.7152, 0.0722));

        // Trace: bright where |wave_y - luma| < thickness
        float dist   = abs(wave_y - luma);
        float trace  = exp(-dist * dist * uResolution.y / max(u_persistence, 0.1));
        float bright = trace * u_intensity;

        // Draw on black background
        fragColor = vec4(bright, bright, bright, 1.0);

    } else if (mode == 1) {
        // RGB parade — three columns (R left, G centre, B right)
        float col_w = 1.0 / 3.0;
        int   col_idx = int(uv.x / col_w);
        float col_uv_x = mod(uv.x, col_w) / col_w; // local X within each section

        // Sample source at this local X position
        vec2 src_uv = vec2(col_uv_x, 0.5);
        vec3 src    = texture(iChannel0, src_uv).rgb;

        float level;
        vec3  trace_col;
        if (col_idx == 0) {
            level = src.r;
            trace_col = vec3(1.0, 0.2, 0.2); // Red trace
        } else if (col_idx == 1) {
            level = src.g;
            trace_col = vec3(0.2, 1.0, 0.2); // Green trace
        } else {
            level = src.b;
            trace_col = vec3(0.2, 0.5, 1.0); // Blue trace
        }

        float dist   = abs(wave_y - level);
        float trace  = exp(-dist * dist * uResolution.y / max(u_persistence, 0.1));
        fragColor    = vec4(trace_col * trace * u_intensity, 1.0);

    } else {
        // Overlay mode: draw source image with luma trace overlaid
        vec4  src_px = texture(iChannel0, uv);
        float luma   = dot(src_px.rgb, vec3(0.2126, 0.7152, 0.0722));
        float dist   = abs(wave_y - luma);
        float trace  = exp(-dist * dist * uResolution.y / max(u_persistence * 0.5, 0.1));
        vec3  overlay = mix(src_px.rgb, vec3(0.0, 1.0, 0.8), trace * u_intensity);
        fragColor     = vec4(overlay, 1.0);
    }
}
