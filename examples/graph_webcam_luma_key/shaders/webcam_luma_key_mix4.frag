#version 330 core
in vec2 v_uv;
out vec4 FragColor;

uniform sampler2D uInput0; // webcam (after passthrough)
uniform sampler2D uInput1; // layer A
uniform sampler2D uInput2; // layer B
uniform sampler2D uInput3; // layer C

// Provided by MatrixMix4 via NodeProps.matrix_params -> uWeights
// x = key_low, y = key_high, z = cam_gain, w = bg_gain
uniform vec4 uWeights;

void main() {
    vec4 cam = texture(uInput0, v_uv);
    vec4 a   = texture(uInput1, v_uv);
    vec4 b   = texture(uInput2, v_uv);
    vec4 c   = texture(uInput3, v_uv);

    // Aggregate non-camera layers as a background field
    vec4 bg = (a + b + c) / 3.0;

    float key_low  = uWeights.x;
    float key_high = uWeights.y;
    float cam_gain = uWeights.z;
    float bg_gain  = uWeights.w;

    // Luma from webcam
    float luma = dot(cam.rgb, vec3(0.299, 0.587, 0.114));

    // Luma key alpha
    float alpha = smoothstep(key_low, key_high, luma);

    vec4 cam_adj = cam * cam_gain;
    vec4 bg_adj  = bg * bg_gain;

    FragColor = mix(bg_adj, cam_adj, alpha);
}
