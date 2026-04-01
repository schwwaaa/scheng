// overlay.frag — FPS and stats overlay
//
// Composites performance and camera data on top of the scene.
// Uses SDF-based digit rendering — no texture lookups, no font files.
//
// Uniforms:
//   iChannel0      = rendered scene
//   u_fps          = frames per second (passed from Rust)
//   u_ms           = frame time in milliseconds
//   u_cam_angle    = camera orbit (0–1)
//   u_cam_elevation= camera height (0–1)
//   u_cam_distance = camera zoom (0–1)

uniform float u_fps;
uniform float u_ms;
uniform float u_cam_angle;
uniform float u_cam_elevation;
uniform float u_cam_distance;

// ── SDF segment display (7-segment style) ─────────────────────────────────
//
// Each digit is drawn using 7 line segments (like an LCD display).
// sdSegment returns the distance from point p to a line from a→b.

float sdSegment(vec2 p, vec2 a, vec2 b) {
    vec2 pa = p - a, ba = b - a;
    float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

// Draw one 7-segment digit.
// p     = pixel position (local to digit, origin bottom-left)
// d     = digit 0–9
// sz    = digit size (width x height)
// thick = segment thickness
float seg7(vec2 p, int d, vec2 sz, float thick) {
    float w = sz.x, h = sz.y;
    float t = thick;
    float dist = 1e9;

    // Segment coordinates (a=bottom-left origin)
    vec2 tl = vec2(0.0, h);       // top-left
    vec2 tr = vec2(w,   h);       // top-right
    vec2 ml = vec2(0.0, h * 0.5); // mid-left
    vec2 mr = vec2(w,   h * 0.5); // mid-right
    vec2 bl = vec2(0.0, 0.0);     // bottom-left
    vec2 br = vec2(w,   0.0);     // bottom-right

    // 7 segments:  a=top  b=top-right  c=bot-right  d=bottom
    //              e=bot-left  f=top-left  g=middle
    //
    // Which segments are ON for each digit (bitmask: gfedcba):
    //   0=0x3F  1=0x06  2=0x5B  3=0x4F  4=0x66
    //   5=0x6D  6=0x7D  7=0x07  8=0x7F  9=0x6F
    int segs[10];
    segs[0] = 0x3F; segs[1] = 0x06; segs[2] = 0x5B; segs[3] = 0x4F;
    segs[4] = 0x66; segs[5] = 0x6D; segs[6] = 0x7D; segs[7] = 0x07;
    segs[8] = 0x7F; segs[9] = 0x6F;

    int mask = segs[clamp(d, 0, 9)];
    float gap = t * 0.8; // small gap at segment ends for realism

    // a = top horizontal
    if ((mask & 1) != 0)
        dist = min(dist, sdSegment(p, tl + vec2(gap, 0), tr - vec2(gap, 0)));
    // b = top-right vertical
    if ((mask & 2) != 0)
        dist = min(dist, sdSegment(p, tr - vec2(0, gap), mr + vec2(0, gap)));
    // c = bottom-right vertical
    if ((mask & 4) != 0)
        dist = min(dist, sdSegment(p, mr - vec2(0, gap), br + vec2(0, gap)));
    // d = bottom horizontal
    if ((mask & 8) != 0)
        dist = min(dist, sdSegment(p, bl + vec2(gap, 0), br - vec2(gap, 0)));
    // e = bottom-left vertical
    if ((mask & 16) != 0)
        dist = min(dist, sdSegment(p, ml - vec2(0, gap), bl + vec2(0, gap)));
    // f = top-left vertical
    if ((mask & 32) != 0)
        dist = min(dist, sdSegment(p, tl - vec2(0, gap), ml + vec2(0, gap)));
    // g = middle horizontal
    if ((mask & 64) != 0)
        dist = min(dist, sdSegment(p, ml + vec2(gap, 0), mr - vec2(gap, 0)));

    return smoothstep(t, t * 0.4, dist);
}

// Draw an integer value (up to 3 digits) at fragcoord position pos
float drawNumber(int val, vec2 fc, vec2 pos, vec2 charSz, float spacing, float thick) {
    float result = 0.0;
    int d2 = (val / 100) % 10;
    int d1 = (val / 10)  % 10;
    int d0 =  val        % 10;

    if (val >= 100)
        result += seg7(fc - pos,                      d2, charSz, thick);
    if (val >= 10)
        result += seg7(fc - pos - vec2(spacing, 0.0), d1, charSz, thick);
        result += seg7(fc - pos - vec2(val >= 10 ? spacing * 2.0 : 0.0, 0.0),
                       d0, charSz, thick);
    return clamp(result, 0.0, 1.0);
}

// ── Progress bar ─────────────────────────────────────────────────────────
float progressBar(vec2 fc, vec2 pos, vec2 sz, float fill) {
    // Background track
    float inTrack = step(pos.x, fc.x) * step(fc.x, pos.x + sz.x)
                  * step(pos.y, fc.y) * step(fc.y, pos.y + sz.y);
    float inFill  = step(pos.x, fc.x) * step(fc.x, pos.x + sz.x * fill) * inTrack;
    return inTrack * 0.3 + inFill * 0.7; // track dim + fill bright
}

// ── Main ──────────────────────────────────────────────────────────────────
void main() {
    // Flip Y so fc.y=0 is screen top — correct for HUD layout
    vec2 fc    = vec2(v_uv.x * uResolution.x, (1.0 - v_uv.y) * uResolution.y);
    // Y-flip when sampling: scene was rendered expecting blit's Y inversion
    vec4 scene = texture(iChannel0, vec2(v_uv.x, 1.0 - v_uv.y));
    vec4 col   = scene;

    // ── Panel ────────────────────────────────────────────────────────────
    float margin = 14.0;
    float panelW = 164.0;
    float panelH = 116.0;
    vec2  panelPos = vec2(margin, margin);  // top-left corner
    vec2  panelMax = panelPos + vec2(panelW, panelH);

    float inPanel = step(panelPos.x, fc.x) * step(fc.x, panelMax.x)
                  * step(panelPos.y, fc.y) * step(fc.y, panelMax.y);

    // Dark translucent background
    col.rgb = mix(col.rgb, vec3(0.03, 0.04, 0.08) + col.rgb * 0.15, inPanel * 0.88);

    // Border glow
    float bdr = 1.2;
    float onBdr = (
        step(panelPos.x,        fc.x) * step(fc.x, panelPos.x + bdr) +
        step(panelMax.x - bdr,  fc.x) * step(fc.x, panelMax.x)       +
        step(panelPos.y,        fc.y) * step(fc.y, panelPos.y + bdr) +
        step(panelMax.y - bdr,  fc.y) * step(fc.y, panelMax.y)
    ) * inPanel;
    col.rgb = mix(col.rgb, vec3(0.2, 0.45, 1.0), clamp(onBdr, 0.0, 1.0) * 0.75);

    // ── FPS number (large, top of panel) ─────────────────────────────────
    vec2  charSz  = vec2(18.0, 28.0);
    float spacing = 20.0;
    float thick   = 2.2;

    // Right-align 3 digits
    vec2  fpsOrigin = vec2(panelMax.x - 14.0 - spacing * 2.0, panelMax.y - 38.0);

    int   ifps = int(u_fps + 0.5);
    float fpsMask = drawNumber(ifps, fc, fpsOrigin, charSz, spacing, thick);

    // Green > 55fps, amber 30–55, red < 30
    vec3  fpsCol = u_fps > 55.0 ? vec3(0.15, 1.0, 0.4)
                 : u_fps > 30.0 ? vec3(1.0, 0.72, 0.08)
                 :                vec3(1.0, 0.18, 0.18);
    col.rgb = mix(col.rgb, fpsCol, fpsMask);

    // Small "fps" label (dot pattern — 3 tiny squares)
    vec2  lp = vec2(panelPos.x + 10.0, panelMax.y - 24.0);
    float labelMask =
        step(lp.x,        fc.x) * step(fc.x, lp.x + 3.0)  * step(lp.y, fc.y) * step(fc.y, lp.y + 3.0) +
        step(lp.x + 5.0,  fc.x) * step(fc.x, lp.x + 8.0)  * step(lp.y, fc.y) * step(fc.y, lp.y + 3.0) +
        step(lp.x + 10.0, fc.x) * step(fc.x, lp.x + 13.0) * step(lp.y, fc.y) * step(fc.y, lp.y + 3.0);
    col.rgb = mix(col.rgb, vec3(0.4, 0.55, 0.9), clamp(labelMask, 0.0, 1.0) * inPanel);

    // ── ms per frame (small, below FPS) ───────────────────────────────────
    vec2  msCharSz  = vec2(10.0, 16.0);
    float msSpacing = 11.5;
    float msThick   = 1.4;
    vec2  msOrigin  = vec2(panelMax.x - 14.0 - msSpacing * 2.0, panelMax.y - 62.0);

    int   ims    = int(u_ms + 0.5);
    float msMask = drawNumber(ims, fc, msOrigin, msCharSz, msSpacing, msThick);
    col.rgb      = mix(col.rgb, vec3(0.45, 0.65, 1.0), msMask);

    // ── Divider ──────────────────────────────────────────────────────────
    float divY   = panelMax.y - 70.0;
    float divMask = step(panelPos.x + 8.0, fc.x) * step(fc.x, panelMax.x - 8.0)
                  * step(divY, fc.y) * step(fc.y, divY + 0.8);
    col.rgb = mix(col.rgb, vec3(0.2, 0.3, 0.5), divMask * inPanel);

    // ── Camera progress bars ──────────────────────────────────────────────
    float barW   = panelW - 20.0;
    float barH   = 4.5;
    float barX   = panelPos.x + 10.0;
    float barGap = 11.0;
    float barY0  = panelPos.y + 10.0;

    // Orbit
    vec2 bar0pos = vec2(barX, barY0);
    vec2 bar0sz  = vec2(barW, barH);
    float b0 = progressBar(fc, bar0pos, bar0sz, u_cam_angle);
    vec3  c0 = mix(vec3(0.1, 0.15, 0.3), vec3(0.25, 0.55, 1.0), b0);
    col.rgb  = mix(col.rgb, c0, clamp(b0, 0.0, 1.0) * inPanel);

    // Elevation
    vec2 bar1pos = vec2(barX, barY0 + barH + barGap);
    float b1 = progressBar(fc, bar1pos, bar0sz, u_cam_elevation);
    vec3  c1 = mix(vec3(0.1, 0.25, 0.15), vec3(0.15, 0.9, 0.45), b1);
    col.rgb  = mix(col.rgb, c1, clamp(b1, 0.0, 1.0) * inPanel);

    // Distance
    vec2 bar2pos = vec2(barX, barY0 + (barH + barGap) * 2.0);
    float b2 = progressBar(fc, bar2pos, bar0sz, u_cam_distance);
    vec3  c2 = mix(vec3(0.25, 0.15, 0.1), vec3(1.0, 0.5, 0.15), b2);
    col.rgb  = mix(col.rgb, c2, clamp(b2, 0.0, 1.0) * inPanel);

    fragColor = col;
}
