// feedback.frag — temporal feedback with luma key
//
// iChannel0 = generator output (current frame content)
// iChannel1 = PreviousFrame output (last composite frame — the feedback buffer)
//
// Signal chain per pixel, each frame:
//   1. Sample and spatially transform previous frame (zoom + rotation)
//   2. Apply decay — attenuate feedback toward black
//   3. Apply hue rotation — colours drift over time
//   4. Compute luma key from generator
//   5. Composite: feedback where dark, generator where bright
//   6. Apply blend mode
//
// CC5 = feedback decay        (0–1 → 0.80–0.998, higher = longer trails)
// CC6 = zoom per frame        (slow zoom in/out creates vortex / infinite zoom)
// CC7 = rotation per frame    (slow rotation creates spiral trails)
// CC8 = hue drift per frame   (trails change colour over time)
//
// LUMA KEY (from generator):
// CC5 controls the brightness threshold in the generator at which the
// new frame keys through the feedback buffer.
// → High threshold: only very bright shapes cut through — thin crisp trails
// → Low threshold:  most content cuts through — fast trail absorption

uniform float u_decay;       // CC5  feedback decay (0–1)
uniform float u_zoom;        // CC6  zoom per frame
uniform float u_rotation;    // CC7  rotation per frame
uniform float u_hue_drift;   // CC8  hue shift per frame

// Luma key params (from gen shader — read as separate uniforms)
uniform float u_luma_thresh; // not MIDI, set in Rust — adjust in main.rs
uniform float u_luma_soft;   // not MIDI, set in Rust — adjust in main.rs

// ── Smooth sampling ──────────────────────────────────────────────────────
// Bicubic-ish smooth kernel for previous frame sampling.
// Prevents pixelation artifacts in the feedback trail.
vec4 sampleSmooth(sampler2D tex, vec2 uv) {
    vec2 res = uResolution;
    vec2 px  = uv * res - 0.5;
    vec2 fl  = floor(px);
    vec2 fr  = fract(px);
    // Smoothstep interpolation (smoother than linear)
    vec2 sm  = fr * fr * (3.0 - 2.0 * fr);
    vec2 uv0 = (fl + 0.5) / res;
    vec2 uv1 = (fl + 1.5) / res;
    vec4 a   = mix(texture(tex, vec2(uv0.x, uv0.y)),
                   texture(tex, vec2(uv1.x, uv0.y)), sm.x);
    vec4 b   = mix(texture(tex, vec2(uv0.x, uv1.y)),
                   texture(tex, vec2(uv1.x, uv1.y)), sm.x);
    return mix(a, b, sm.y);
}

// ── Hue rotation ─────────────────────────────────────────────────────────
// Rotates hue of an RGB colour by angle (radians) in HSL space.
vec3 rotateHue(vec3 rgb, float angle) {
    float c = cos(angle), s = sin(angle);
    mat3  m = mat3(
        0.213 + c * 0.787 - s * 0.213,
        0.213 - c * 0.213 + s * 0.143,
        0.213 - c * 0.213 - s * 0.787,

        0.715 - c * 0.715 - s * 0.715,
        0.715 + c * 0.285 + s * 0.140,
        0.715 - c * 0.715 + s * 0.715,

        0.072 - c * 0.072 + s * 0.928,
        0.072 - c * 0.072 - s * 0.283,
        0.072 + c * 0.928 + s * 0.072
    );
    return clamp(m * rgb, 0.0, 1.0);
}

// ── ACES tone map ─────────────────────────────────────────────────────────
vec3 aces(vec3 x) {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

// ── Main ─────────────────────────────────────────────────────────────────
void main() {
    vec2 uv = v_uv;

    // ── 1. Sample generator (foreground) ─────────────────────────────────
    vec4 gen = texture(iChannel0, uv);

    // ── 2. Transform UV for previous frame sample ─────────────────────────
    // Zoom from centre — slight inward zoom prevents the trail from being
    // the exact same size every frame, creating a depth effect
    float zoom_factor = 1.0 + (u_zoom - 0.5) * 0.006; // ±0.3% per frame
    vec2  uv_c = uv - 0.5;                               // centre at 0,0

    // Rotation
    float rot_speed = (u_rotation - 0.5) * 0.008;        // ±0.4% of a full turn
    float c_r = cos(rot_speed), s_r = sin(rot_speed);
    uv_c = vec2(c_r * uv_c.x - s_r * uv_c.y,
                s_r * uv_c.x + c_r * uv_c.y);

    // Apply zoom and translate back
    vec2 fb_uv = uv_c / zoom_factor + 0.5;

    // ── 3. Sample previous frame with smooth interpolation ────────────────
    vec4 prev;
    if (fb_uv.x < 0.0 || fb_uv.x > 1.0 || fb_uv.y < 0.0 || fb_uv.y > 1.0) {
        prev = vec4(0.0);  // out-of-bounds = black (no wrap bleeding)
    } else {
        prev = sampleSmooth(iChannel1, fb_uv);
    }

    // ── 4. Decay — attenuate previous frame toward black ──────────────────
    // Map u_decay (0–1) to useful range 0.80–0.998
    // Below 0.80 the feedback dies too fast to be interesting
    float decay = mix(0.80, 0.998, u_decay);
    vec3  fb    = prev.rgb * decay;

    // ── 5. Hue drift on feedback ──────────────────────────────────────────
    // Drift angle accumulates each frame via uFrame counter
    float drift  = (u_hue_drift - 0.5) * 0.04;  // ±0.02 rad/frame at extremes
    fb = rotateHue(fb, drift * float(uFrame % 628));  // wrap at ~100 cycles

    // ── 6. Luma key — composite generator over feedback ───────────────────
    // Rec.709 luma weights (perceptually correct)
    float luma = dot(gen.rgb, vec3(0.2126, 0.7152, 0.0722));

    // Threshold maps: above thresh → generator shows, below → feedback persists
    // u_luma_thresh and u_luma_soft set in Rust (via cfg.uniforms)
    float thresh = mix(0.08, 0.85, u_luma_thresh);
    float soft   = mix(0.02, 0.30, u_luma_soft);
    float mask   = smoothstep(thresh - soft, thresh + soft, luma);

    // Composite: feedback where dark, generator where bright
    vec3 result = mix(fb, gen.rgb, mask);

    // ── 7. Gentle additive boost for bright areas ─────────────────────────
    // Keeps the image from going dark even with high decay
    result = max(result, gen.rgb * mask * 0.3);

    // ── 8. Tone map + gamma ───────────────────────────────────────────────
    result = aces(result);
    result = pow(clamp(result, 0.0, 1.0), vec3(0.4545));  // γ 2.2

    fragColor = vec4(result, 1.0);
}
