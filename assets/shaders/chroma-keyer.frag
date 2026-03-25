// ─────────────────────────────────────────────────────────────────────────────
// scheng shader: chroma-keyer.frag
//
// Chroma Keyer
// Keys on a target hue (green screen, blue screen, or any colour).
// Generates a matte from colour distance and composites fg over bg.
//
// Reference: Broadcast chroma key, LZX colour difference keyer techniques.
//
// Node role:  Processor / Mixer  (2 inputs)
//   iChannel0 = foreground + key colour source (same signal, split internally)
//   iChannel1 = background
//
// Uniforms:
//   u_key_hue      [0, 360]  target hue to key out       default: 120.0 (green)
//   u_hue_range    [0, 180]  hue acceptance range (±)     default: 30.0
//   u_saturation   [0, 1]    min saturation to trigger key default: 0.2
//   u_softness     [0, 1]    edge feather                  default: 0.1
//   u_spill_reduce [0, 1]    green/blue spill suppression  default: 0.5
// ─────────────────────────────────────────────────────────────────────────────
#version 330 core
in  vec2 v_uv;
out vec4 fragColor;

uniform sampler2D iChannel0;  // foreground (with chroma key colour)
uniform sampler2D iChannel1;  // background
uniform float u_key_hue;
uniform float u_hue_range;
uniform float u_saturation;
uniform float u_softness;
uniform float u_spill_reduce;

// RGB → HSV for hue-based keying
vec3 rgb_to_hsv(vec3 c) {
    float cmax = max(c.r, max(c.g, c.b));
    float cmin = min(c.r, min(c.g, c.b));
    float delta = cmax - cmin;

    float h = 0.0;
    if (delta > 0.0001) {
        if      (cmax == c.r) h = 60.0 * mod((c.g - c.b) / delta, 6.0);
        else if (cmax == c.g) h = 60.0 * ((c.b - c.r) / delta + 2.0);
        else                   h = 60.0 * ((c.r - c.g) / delta + 4.0);
    }
    if (h < 0.0) h += 360.0;

    float s = (cmax < 0.0001) ? 0.0 : delta / cmax;
    return vec3(h, s, cmax);
}

void main() {
    vec4 fg = texture(iChannel0, v_uv);
    vec4 bg = texture(iChannel1, v_uv);

    vec3 hsv = rgb_to_hsv(fg.rgb);
    float h  = hsv.x;
    float s  = hsv.y;

    // Hue distance (circular, handles wraparound at 0/360)
    float hue_dist = abs(mod(h - u_key_hue + 180.0, 360.0) - 180.0);

    // Key matte: 1 = keep foreground, 0 = show background
    // Only key pixels that are saturated enough (avoid keying grey/white)
    float sat_mask = smoothstep(u_saturation - 0.05, u_saturation + 0.05, s);
    float hue_mask = 1.0 - smoothstep(
        u_hue_range - u_softness * 20.0,
        u_hue_range + u_softness * 20.0,
        hue_dist
    );
    float key = sat_mask * hue_mask; // 1 = is the key colour

    // Spill suppression: desaturate fg pixels near the key hue
    // (prevents green/blue fringe on edges — standard broadcast technique)
    float spill  = clamp(1.0 - hue_dist / (u_hue_range + 0.001), 0.0, 1.0);
    float luma   = dot(fg.rgb, vec3(0.2126, 0.7152, 0.0722));
    vec3  fg_nospill = mix(fg.rgb, vec3(luma), spill * u_spill_reduce * sat_mask);

    // Composite: bg where key colour detected, fg elsewhere
    fragColor = mix(vec4(fg_nospill, fg.a), bg, key);
}
