# scheng-playground

<p align="center">
  <img src="https://img.shields.io/badge/scheng-template-3b82f6?style=flat-square"/>
  <img src="https://img.shields.io/badge/complexity-intermediate-f59e0b?style=flat-square"/>
  <img src="https://img.shields.io/badge/purpose-explore%20%26%20learn-8b5cf6?style=flat-square"/>
</p>

An interactive multi-shader playground for exploring scheng. Switch between shader programs at runtime, tweak parameters live, and experiment with the graph system — no code changes needed.

---

## What is this?

`scheng-playground` is a development sandbox. Unlike the single-shader templates, it:

- Loads **all `.frag` files** in `assets/shaders/` at startup
- Lets you **cycle between shaders** with keyboard shortcuts
- Exposes **8 MIDI-controlled parameters** (CC1–CC8) wired to `u_p1`–`u_p8` in every shader
- Logs **FPS, resolution, current shader, and parameter values** to the terminal

Use it to:
- Rapidly prototype shader ideas without rebuilding
- Learn how the scheng uniform system works
- Test new shaders before adding them to a real instrument
- Demonstrate the SDK to others

---

## Run

```bash
cargo run --release
```

### Keyboard shortcuts

| Key | Action |
|-----|--------|
| `→` / `]` | Next shader |
| `←` / `[` | Previous shader |
| `R` | Reload all shaders from disk |
| `F` | Toggle fullscreen |
| `Escape` | Quit |

---

## Writing shaders for the playground

Every shader gets the standard scheng uniforms plus 8 playground parameters:

```glsl
// Standard uniforms (always available)
// uTime, uFrame, uResolution, v_uv, fragColor

// Playground parameters — all 0.0–1.0, controlled via MIDI CC1–CC8
uniform float u_p1;  // CC1
uniform float u_p2;  // CC2
uniform float u_p3;  // CC3
uniform float u_p4;  // CC4
uniform float u_p5;  // CC5
uniform float u_p6;  // CC6
uniform float u_p7;  // CC7
uniform float u_p8;  // CC8
```

Example shader using playground params:

```glsl
// assets/shaders/plasma.frag
uniform float u_p1;  // speed
uniform float u_p2;  // scale
uniform float u_p3;  // hue shift

void main() {
    vec2  uv = v_uv * 2.0 - 1.0;
    float t  = uTime * (0.5 + u_p1 * 2.0);
    float s  = 2.0 + u_p2 * 6.0;

    float v  = sin(uv.x * s + t) + sin(uv.y * s - t);
    vec3  col = 0.5 + 0.5 * cos(v * 3.14 + u_p3 * 6.28 + vec3(0, 2, 4));
    fragColor = vec4(col, 1.0);
}
```

Drop any `.frag` file into `assets/shaders/` and press `R` to load it without restarting.

---

## Project layout

```
scheng-playground/
├── Cargo.toml
├── build.rs
├── src/
│   └── main.rs
└── assets/
    └── shaders/
        ├── gradient.frag    ← included starter shaders
        ├── plasma.frag
        ├── tunnel.frag
        ├── solarize.frag
        └── noise.frag
```

---

## MIDI parameter mapping

| CC | Parameter | Default | Range |
|----|-----------|---------|-------|
| CC1 | `u_p1` | 0.5 | 0.0–1.0 |
| CC2 | `u_p2` | 0.5 | 0.0–1.0 |
| CC3 | `u_p3` | 0.0 | 0.0–1.0 |
| CC4 | `u_p4` | 0.0 | 0.0–1.0 |
| CC5 | `u_p5` | 0.0 | 0.0–1.0 |
| CC6 | `u_p6` | 0.0 | 0.0–1.0 |
| CC7 | `u_p7` | 0.0 | 0.0–1.0 |
| CC8 | `u_p8` | 0.0 | 0.0–1.0 |

All values are smoothed (smooth=0.05) so CC changes are silky — no stepping.

---

## Full documentation

[Developer Reference](https://yourusername.github.io/scheng/developer-reference.html)
