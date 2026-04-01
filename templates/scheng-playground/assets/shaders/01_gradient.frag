// 01_gradient.frag — animated color gradient
// The hello world of fragment shaders.
//
// u_p1 = animation speed
// u_p2 = color shift
// u_p3 = pattern frequency

uniform float u_p1;
uniform float u_p2;
uniform float u_p3;
uniform float u_p4;
uniform float u_p5;
uniform float u_p6;
uniform float u_p7;
uniform float u_p8;
void main() {
    float t   = uTime * (0.3 + u_p1 * 1.5);
    float freq = 1.0 + u_p3 * 5.0;
    vec3  col  = 0.5 + 0.5 * cos(t + u_p2 * 6.28 + v_uv.xyx * freq + vec3(0.0, 2.1, 4.2));
    fragColor  = vec4(col, 1.0);
}
