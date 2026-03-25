// ─────────────────────────────────────────────────────────────────────────────
// scheng shader: colorizer.frag
//
// Colorizer
// Maps input luma to a configurable colour gradient — the LZX Chroma Key /
// Colour Encoder style module. Takes a greyscale or colour signal and
// recolours it by mapping the luminance value through a hue cycle.
//
// In analog hardware this is done with a voltage-controlled oscillator
// sweeping the colour subcarrier phase proportional to luma voltage.
//
// Node role:  Processor  (iChannel0 → out)
// Uniforms:
//   u_hue_start   [0, 360]   starting hue for luma=0    default: 0.0
//   u_hue_range   [-360,360] hue sweep across luma range default: 360.0
//   u_saturation  [0, 1]     output chroma saturation    default: 1.0
//   u_luminance   [0, 1]     output luma (brightness)    default: 0.5
//   u_invert      [0, 1]     invert luma before mapping  default: 0.0
// ─────────────────────────────────────────────────────────────────────────────
#version 330 core
in  vec2 v_uv;
out vec4 fragColor;

uniform sampler2D iChannel0;
uniform float u_hue_start;   // hue at luma=0 (degrees)
uniform float u_hue_range;   // degrees of hue swept across full luma range
uniform float u_saturation;  // output saturation
uniform float u_luminance;   // output luminance
uniform float u_invert;      // 0=normal, 1=invert luma before mapping

// HSL → RGB (classic formula, faithful to colour encoder hardware output)
vec3 hsl_to_rgb(float h, float s, float l) {
    float c = (1.0 - abs(2.0 * l - 1.0)) * s;
    h = mod(h / 60.0, 6.0);
    float x = c * (1.0 - abs(mod(h, 2.0) - 1.0));
    vec3 rgb;
    if      (h < 1.0) rgb = vec3(c, x, 0);
    else if (h < 2.0) rgb = vec3(x, c, 0);
    else if (h < 3.0) rgb = vec3(0, c, x);
    else if (h < 4.0) rgb = vec3(0, x, c);
    else if (h < 5.0) rgb = vec3(x, 0, c);
    else              rgb = vec3(c, 0, x);
    float m = l - 0.5 * c;
    return clamp(rgb + m, 0.0, 1.0);
}

void main() {
    vec4  src  = texture(iChannel0, v_uv);

    // Extract luma (Rec.709 coefficients)
    float luma = dot(src.rgb, vec3(0.2126, 0.7152, 0.0722));

    // Optional inversion
    luma = mix(luma, 1.0 - luma, u_invert);

    // Map luma → hue
    float hue = mod(u_hue_start + luma * u_hue_range, 360.0);

    vec3 colour = hsl_to_rgb(hue, u_saturation, u_luminance);

    fragColor = vec4(colour, src.a);
}
