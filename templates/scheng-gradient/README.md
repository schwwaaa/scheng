# scheng-gradient

Minimal [scheng](https://github.com/your-org/scheng) instrument.

Opens a window and hot-reloads a GLSL fragment shader from `assets/shaders/main.frag`.
Edit and save the shader — the output updates immediately.

## Requirements

- Rust 1.75+
- macOS (Metal) / Windows (DX12) / Linux (Vulkan)
- macOS only: `vendor/Syphon.framework` in the scheng workspace (for Syphon I/O)

## Setup

1. Clone scheng next to this project:
   ```
   projects/
     scheng/
     scheng-gradient/    ← this project
   ```

2. Build and run:
   ```bash
   cargo run --release
   ```

## Run options

```bash
# Custom resolution
cargo run --release -- --width 1920 --height 1080

# 4K
cargo run --release -- --width 3840 --height 2160

# 4K + MSAA 4x anti-aliasing
cargo run --release -- --width 3840 --height 2160 --msaa 4

# Stream to RTMP (requires mediamtx or similar)
cargo run --release -- --stream rtmp://localhost:1935/live/key

# Stream to RTSP
cargo run --release -- --stream rtsp://localhost:8554/live

# Record to file
cargo run --release -- --record output.mp4
```

## Shader

Edit `assets/shaders/main.frag` — changes are picked up live.

Available built-in uniforms:
| Uniform | Type | Description |
|---------|------|-------------|
| `uTime` | `float` | Seconds since start |
| `uFrame` | `float` | Frame counter |
| `uResolution` | `vec2` | Width × height in pixels |
| `iChannel0–3` | `sampler2D` | Input textures (when I/O enabled) |

Custom `u_*` uniforms are automatically exposed via OSC and MIDI CC.

## Adding I/O

Uncomment the relevant sections in `Cargo.toml` and `src/main.rs`:

| Feature | What it enables |
|---------|----------------|
| `midi`  | MIDI CC → uniform control |
| `osc`   | OSC → uniform control |
| `syphon` | Syphon output (macOS) |
| `stream` | RTMP/RTSP/file output |

## Architecture

```
assets/shaders/main.frag
        ↓ hot-reload
   ShaderSource node
        ↓ wgpu render
   PixelsOut node
        ↓
   PreviewSink → window
```
