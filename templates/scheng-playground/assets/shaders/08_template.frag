// 08_template.frag — blank starting point
// Copy this file, rename it, start writing.
// All 8 params are available — declare what you use.
//
// u_p1 = (your parameter)
// u_p2 = (your parameter)
// ...

uniform float u_p1;
uniform float u_p2;
uniform float u_p3;
uniform float u_p4;
uniform float u_p5;
uniform float u_p6;
uniform float u_p7;
uniform float u_p8;
void main() {
    vec2  uv  = v_uv;                     // [0,1] × [0,1]
    float t   = uTime;                    // seconds since start
    vec2  res = uResolution;              // width × height

    // -- your shader here --

    vec3  col = vec3(uv, 0.5 + 0.5 * sin(t));
    fragColor = vec4(col, 1.0);
}
