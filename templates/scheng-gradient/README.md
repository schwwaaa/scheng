# scheng-gradient

<p align="center">
  <img src="https://img.shields.io/badge/scheng-template-3b82f6?style=flat-square"/>
  <img src="https://img.shields.io/badge/complexity-minimal-10b981?style=flat-square"/>
  <img src="https://img.shields.io/badge/hot--reload-yes-06b6d4?style=flat-square"/>
</p>

The minimal scheng starter template. One shader, hot-reload, no I/O dependencies.

This is the fastest path from zero to a GPU-rendered window. Start here, then add what you need.

---

## Run

```bash
cargo run --release
```

```bash
# 4K with MSAA
cargo run --release -- --width 3840 --height 2160 --msaa 4
```

---

## How it works

The instrument opens a window and renders `assets/shaders/main.frag` every frame. Edit the shader file and save — the window updates instantly.

```glsl
// assets/shaders/main.frag
void main() {
    vec2  uv  = v_uv;
    float t   = uTime;
    vec3  col = 0.5 + 0.5 * cos(t + uv.xyx + vec3(0, 2, 4));
    fragColor = vec4(col, 1.0);
}
```

**Built-in shader uniforms** — always available, no declaration needed:

| Uniform | Type | Value |
|---------|------|-------|
| `uTime` | float | Seconds since start |
| `uFrame` | uint | Frame counter |
| `uResolution` | vec2 | Width × height |
| `v_uv` | vec2 | UV coordinates [0,1] |
| `fragColor` | vec4 out | Write your pixel here |

---

## Project layout

```
scheng-gradient/
├── Cargo.toml
├── build.rs
├── src/
│   └── main.rs
└── assets/
    └── shaders/
        └── main.frag    ← edit this
```

Place this project next to the `scheng/` workspace:

```
projects/
  scheng/             ← SDK workspace
  scheng-gradient/    ← this project
```

---

## Next steps

Add MIDI control → **`scheng-processor`**
Add Syphon I/O → **`scheng-mixer`**
Add video files → **`scheng-video-mixer`**

Full documentation: [Developer Reference](https://yourusername.github.io/scheng/developer-reference.html)
