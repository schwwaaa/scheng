// ─────────────────────────────────────────────────────────────────────────────
// scheng shader: proc-amp.frag
//
// Proc Amp (Processing Amplifier)
// Broadcast-standard video processing: brightness, contrast, saturation, hue.
// Reference: Extron, Kramer, and analog video sync processors.
//
// In analog video hardware, a proc amp regenerates sync, adjusts luma
// amplitude (brightness/contrast) and adjusts chroma (saturation/hue).
// This shader replicates the signal-chain math in GLSL.
//
// Node role:  Processor  (iChannel0 → out)
// Uniforms:
//   u_brightness  [-1, 1]   additive luma offset       default: 0.0
//   u_contrast    [0, 3]    luma gain multiplier        default: 1.0
//   u_saturation  [0, 3]    chroma amplitude scale      default: 1.0
//   u_hue         [-180,180] hue rotation in degrees    default: 0.0
// ─────────────────────────────────────────────────────────────────────────────
#version 330 core
in  vec2 v_uv;
out vec4 fragColor;

uniform sampler2D iChannel0;
uniform float u_brightness;   // additive offset to luma
uniform float u_contrast;     // luma gain (1.0 = unity)
uniform float u_saturation;   // chroma scale (1.0 = unity)
uniform float u_hue;          // hue rotation in degrees

// ── RGB ↔ YIQ (NTSC colour space) ────────────────────────────────────────
// YIQ separates luma (Y) from chroma (I,Q) exactly as broadcast proc amps do.
// Using YIQ rather than HSV matches analog hardware more closely.

vec3 rgb_to_yiq(vec3 c) {
    return vec3(
         0.2990 * c.r + 0.5870 * c.g + 0.1140 * c.b,
         0.5959 * c.r - 0.2746 * c.g - 0.3213 * c.b,
         0.2115 * c.r - 0.5227 * c.g + 0.3112 * c.b
    );
}

vec3 yiq_to_rgb(vec3 yiq) {
    return clamp(vec3(
        yiq.x + 0.9563 * yiq.y + 0.6210 * yiq.z,
        yiq.x - 0.2721 * yiq.y - 0.6474 * yiq.z,
        yiq.x - 1.1070 * yiq.y + 1.7046 * yiq.z
    ), 0.0, 1.0);
}

void main() {
    vec4 src = texture(iChannel0, v_uv);
    vec3 yiq = rgb_to_yiq(src.rgb);

    // 1. Contrast: scale luma around mid-grey (0.5), matching analog gain stage
    yiq.x = (yiq.x - 0.5) * u_contrast + 0.5;

    // 2. Brightness: additive luma offset (IRE shift)
    yiq.x = yiq.x + u_brightness;

    // 3. Saturation: scale chroma vector amplitude
    yiq.yz *= u_saturation;

    // 4. Hue rotation: rotate the IQ chroma vector
    //    Hardware hue control rotates the colour subcarrier phase
    float angle = radians(u_hue);
    float cosA  = cos(angle);
    float sinA  = sin(angle);
    float i     = yiq.y * cosA - yiq.z * sinA;
    float q     = yiq.y * sinA + yiq.z * cosA;
    yiq.yz      = vec2(i, q);

    fragColor = vec4(yiq_to_rgb(yiq), src.a);
}
