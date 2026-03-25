// scheng instrument shader — main.frag
//
// Edit this file while the instrument is running.
// It hot-reloads automatically within ~100ms.
//
// Uniforms are declared here and mapped in assets/params.json.
// Add a uniform here + a matching entry in params.json to get a live slider.

#version 330 core
in  vec2 v_uv;
out vec4 fragColor;

uniform sampler2D iChannel0;   // input source (Syphon / video / webcam)
uniform float uTime;
uniform vec2  uResolution;

// Live parameters — controlled via MIDI CC / OSC / sliders
uniform float u_speed;         // animation speed
uniform float u_brightness;    // output brightness
uniform float u_hue_shift;     // hue rotation (degrees)

// ── YIQ hue rotation ─────────────────────────────────────────────────────
vec3 hue_rotate(vec3 c, float degrees) {
    float a = radians(degrees);
    float y  =  0.2990 * c.r + 0.5870 * c.g + 0.1140 * c.b;
    float i  =  0.5959 * c.r - 0.2746 * c.g - 0.3213 * c.b;
    float q  =  0.2115 * c.r - 0.5227 * c.g + 0.3112 * c.b;
    float i2 =  i * cos(a) - q * sin(a);
    float q2 =  i * sin(a) + q * cos(a);
    return clamp(vec3(
        y + 0.9563 * i2 + 0.6210 * q2,
        y - 0.2721 * i2 - 0.6474 * q2,
        y - 1.1070 * i2 + 1.7046 * q2
    ), 0.0, 1.0);
}

void main() {
    vec2 uv = v_uv;

    // Animated gradient when no input is connected
    float t = uTime * u_speed;
    vec3 col = vec3(
        0.5 + 0.5 * sin(t        + uv.x * 3.14159),
        0.5 + 0.5 * sin(t * 0.7  + uv.y * 3.14159 + 1.0),
        0.5 + 0.5 * sin(t * 0.5  + (uv.x + uv.y) * 2.0 + 2.0)
    );

    // Blend with iChannel0 if connected
    vec4 src = texture(iChannel0, uv);
    col = mix(col, src.rgb, src.a);

    // Apply live parameters
    col = hue_rotate(col, u_hue_shift);
    col *= u_brightness;

    fragColor = vec4(col, 1.0);
}
