// passthrough.frag — pass iChannel0 straight through.
// Used for Syphon input nodes to inject external texture into the graph.
void main() {
    fragColor = texture(iChannel0, vec2(v_uv.x, 1.0 - v_uv.y));
}
