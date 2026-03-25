// proc_amp.frag — Proc-amp (processing amplifier)
//
// Classic analog video signal processor controls:
//   u_brightness  — black level lift/cut    (-1.0 to +1.0, default 0.0)
//   u_contrast    — gain around 0.5          (0.0 to 3.0,   default 1.0)
//   u_saturation  — color intensity           (0.0 to 3.0,   default 1.0)
//   u_hue         — hue rotation in degrees  (-180 to +180,  default 0.0)
//
// iChannel0 = webcam / Syphon input

uniform float u_brightness;
uniform float u_contrast;
uniform float u_saturation;
uniform float u_hue;

// RGB <-> HSV helpers
vec3 rgb2hsv(vec3 c) {
    vec4 K = vec4(0.0, -1.0/3.0, 2.0/3.0, -1.0);
    vec4 p = mix(vec4(c.bg, K.wz), vec4(c.gb, K.xy), step(c.b, c.g));
    vec4 q = mix(vec4(p.xyw, c.r), vec4(c.r, p.yzx), step(p.x, c.r));
    float d = q.x - min(q.w, q.y);
    float e = 1.0e-10;
    return vec3(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}

vec3 hsv2rgb(vec3 c) {
    vec4 K = vec4(1.0, 2.0/3.0, 1.0/3.0, 3.0);
    vec3 p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
    return c.z * mix(K.xxx, clamp(p - K.xxx, 0.0, 1.0), c.y);
}

void main() {
    vec4 src = texture(iChannel0, vec2(v_uv.x, 1.0 - v_uv.y));
    vec3 col = src.rgb;

    // Brightness — lift/cut the black level
    col += u_brightness;

    // Contrast — scale around 0.5 (analog gain)
    col = (col - 0.5) * u_contrast + 0.5;

    // Saturation + Hue via HSV
    vec3 hsv = rgb2hsv(clamp(col, 0.0, 1.0));
    hsv.x = fract(hsv.x + u_hue / 360.0);  // hue rotation
    hsv.y = clamp(hsv.y * u_saturation, 0.0, 1.0);  // saturation
    col = hsv2rgb(hsv);

    fragColor = vec4(clamp(col, 0.0, 1.0), src.a);
}
