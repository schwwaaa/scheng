// ─────────────────────────────────────────────────────────────────────────────
// scheng shader: vectorscope.frag
//
// Vectorscope
// Plots each pixel as a point on a Cb/Cr (or IQ) colour plane.
// The angle = hue, the radius = saturation. Used for calibrating colour
// balance, checking skin tone line, and legality in broadcast work.
//
// Reference: Tektronix 1760 vectorscope, TSL PTM001, Leader VC-5800.
//
// The vectorscope reads the entire source frame and accumulates each pixel's
// chroma onto the plot plane — bright spots = common colours in the image.
// Reference targets (colour bar dots) are shown at their standard positions.
//
// Node role:  Processor  (iChannel0 = signal to analyse)
// Uniforms:
//   u_gain       [0.1, 4]  plot gain (brighten sparse plots)   default: 1.0
//   u_graticule  [0, 1]    show reference targets and axes     default: 1.0
//   u_skin_line  [0, 1]    show the broadcast skin-tone line   default: 1.0
// ─────────────────────────────────────────────────────────────────────────────
#version 330 core
in  vec2 v_uv;
out vec4 fragColor;

uniform sampler2D iChannel0;
uniform float u_gain;
uniform float u_graticule;
uniform float u_skin_line;
uniform vec2  uResolution;

// RGB → YCbCr (BT.601 digital)
vec3 rgb_to_ycbcr(vec3 c) {
    float y  =  0.2990  * c.r + 0.5870  * c.g + 0.1140  * c.b;
    float cb = -0.16875 * c.r - 0.33126 * c.g + 0.50001 * c.b;
    float cr =  0.50001 * c.r - 0.41869 * c.g - 0.08132 * c.b;
    return vec3(y, cb, cr);
}

// Draw a small dot at a given Cb/Cr position
float dot_at(vec2 uv_plot, vec2 target, float size) {
    return smoothstep(size, size * 0.5, length(uv_plot - target));
}

void main() {
    // Map this fragment's UV to the Cb/Cr plot plane [-0.5, 0.5]
    vec2 uv_plot = (v_uv - 0.5);  // -0.5 to 0.5

    // ── Accumulate: sample the source image to plot chroma points ─────────
    // We sample a grid of source pixels and check if their Cb/Cr falls
    // near this plot position. This is a simplified single-pass approach.
    float intensity = 0.0;
    const int SAMPLES = 32;
    for (int i = 0; i < SAMPLES; i++) {
        for (int j = 0; j < SAMPLES; j++) {
            vec2  src_uv = vec2(float(i), float(j)) / float(SAMPLES);
            vec3  ycbcr  = rgb_to_ycbcr(texture(iChannel0, src_uv).rgb);
            vec2  chroma = ycbcr.yz;  // Cb, Cr

            // Distance from this chroma point to current plot position
            float d = length(chroma - uv_plot);
            // Gaussian accumulation
            intensity += exp(-d * d * uResolution.x * 0.5 / u_gain);
        }
    }
    intensity = clamp(intensity / float(SAMPLES * SAMPLES) * u_gain * 8.0, 0.0, 1.0);

    // Plot colour: false colour based on chroma angle (hue) of this plot position
    float hue_angle = atan(uv_plot.y, uv_plot.x);
    float hue_norm  = hue_angle / (2.0 * 3.14159265) + 0.5;
    float r = clamp(abs(mod(hue_norm * 6.0 - 3.0, 6.0) - 3.0) - 1.0, 0.0, 1.0);
    float g = clamp(2.0 - abs(mod(hue_norm * 6.0 - 2.0, 6.0) - 2.0), 0.0, 1.0);
    float b = clamp(2.0 - abs(mod(hue_norm * 6.0 - 4.0, 6.0) - 2.0), 0.0, 1.0);
    vec3 plot_col = mix(vec3(intensity), vec3(r, g, b) * intensity, 0.7);

    // ── Graticule ─────────────────────────────────────────────────────────
    float grat = 0.0;
    if (u_graticule > 0.5) {
        // Crosshairs (axes)
        float axis_w = 0.003;
        grat += smoothstep(axis_w, 0.0, abs(uv_plot.x));
        grat += smoothstep(axis_w, 0.0, abs(uv_plot.y));

        // Outer circle at 0.5 radius (100% saturation)
        float ring_r = length(uv_plot);
        grat += smoothstep(0.005, 0.0, abs(ring_r - 0.5)) * 0.4;
        // Inner circle at 0.25 (50% saturation)
        grat += smoothstep(0.005, 0.0, abs(ring_r - 0.25)) * 0.2;

        // SMPTE colour bar reference targets (Cb/Cr positions)
        // Yellow, Cyan, Green, Magenta, Red, Blue
        grat += dot_at(uv_plot, vec2(-0.166,  0.417), 0.015) * 0.8; // Yellow
        grat += dot_at(uv_plot, vec2(-0.425, -0.095), 0.015) * 0.8; // Cyan
        grat += dot_at(uv_plot, vec2(-0.259, -0.512), 0.015) * 0.8; // Green
        grat += dot_at(uv_plot, vec2( 0.259,  0.512), 0.015) * 0.8; // Magenta
        grat += dot_at(uv_plot, vec2( 0.425,  0.095), 0.015) * 0.8; // Red
        grat += dot_at(uv_plot, vec2( 0.166, -0.417), 0.015) * 0.8; // Blue

        grat = clamp(grat, 0.0, 0.5);
    }

    // ── Skin tone line ────────────────────────────────────────────────────
    float skin = 0.0;
    if (u_skin_line > 0.5) {
        // Broadcast skin tone line: approximately 123° hue in IQ / YCbCr
        // Represented as a line from origin at this angle
        vec2  skin_dir = normalize(vec2(0.382, -0.261)); // ~123° in Cb/Cr
        float d_skin   = abs(dot(uv_plot, vec2(-skin_dir.y, skin_dir.x)));
        float along    = dot(uv_plot, skin_dir);
        skin = smoothstep(0.006, 0.0, d_skin) * step(0.0, along) * 0.6;
    }

    vec3  final_col = plot_col
                    + vec3(0.15, 0.15, 0.15) * grat
                    + vec3(1.0, 0.8, 0.3) * skin;

    fragColor = vec4(clamp(final_col, 0.0, 1.0), 1.0);
}
