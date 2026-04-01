// 02_plasma.frag — layered sine-wave plasma
// Classic demo scene effect — multiple interference waves.
//
// u_p1 = animation speed
// u_p2 = scale
// u_p3 = hue rotation
// u_p4 = layer count blend

uniform float u_p1;
uniform float u_p2;
uniform float u_p3;
uniform float u_p4;
uniform float u_p5;
uniform float u_p6;
uniform float u_p7;
uniform float u_p8;
void main() {
    float t   = uTime * (0.4 + u_p1 * 1.2);
    vec2  uv  = v_uv * (2.0 + u_p2 * 6.0);

    float v   = sin(uv.x + t);
    v        += sin(uv.y + t * 0.8);
    v        += sin((uv.x + uv.y) * 0.7 + t * 1.1);

    float cx  = uv.x + 0.5 * sin(t * 0.3);
    float cy  = uv.y + 0.5 * cos(t * 0.4);
    v        += sin(sqrt(cx*cx + cy*cy) * (3.0 + u_p4 * 5.0) - t * 1.5);

    vec3 col;
    float phase = u_p3 * 6.28;
    col.r = sin(v * 3.14 + phase)            * 0.5 + 0.5;
    col.g = sin(v * 3.14 + phase + 2.094)    * 0.5 + 0.5;
    col.b = sin(v * 3.14 + phase + 4.189)    * 0.5 + 0.5;
    col   = pow(col, vec3(1.4));

    fragColor = vec4(col, 1.0);
}
