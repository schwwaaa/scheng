<p align="center">
  <img width="35%" height="35%" src="https://raw.githubusercontent.com/schwwaaa/scheng/refs/heads/main/img/logo.png"/>  
</p>

<p align="center"><em>GPU-accelerated video synthesis SDK for Rust</em></p> 

<p align="center">
  <img src="https://img.shields.io/badge/version-0.1.0-3b82f6?style=flat-square"/>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey?style=flat-square"/>
  <img src="https://img.shields.io/badge/backend-Metal%20%7C%20DX12%20%7C%20Vulkan-8b5cf6?style=flat-square"/>
  <img src="https://img.shields.io/badge/Rust-1.75%2B-orange?style=flat-square&logo=rust"/>
  <img src="https://img.shields.io/badge/wgpu-23-06b6d4?style=flat-square"/>
  <img src="https://img.shields.io/badge/license-MIT-10b981?style=flat-square"/>
</p>



---

## What is scheng?

scheng is a Rust engine and SDK for building programmable video instruments. Rather than a fixed VJ application, it gives you the building blocks: a graph-based execution model, explicit shader contracts, and a modular I/O surface. You wire them into your own executable.

The engine is deliberately minimal — it evaluates frames deterministically. It does not own time, schedule execution, or manage devices. Those responsibilities belong to your instrument.

Build real-time shader instruments — live mixers, processors, and generators — with professional video I/O built in.

```bash
cargo run --release -- --width 1920 --height 1080 --msaa 4
```

---

## Features

- **GPU-native** — Rgba16Float render pipeline on Metal, DX12, and Vulkan via wgpu
- **Hot-reload shaders** — edit GLSL, save, see — zero restart
- **MIDI & OSC** — CC messages wire directly to shader uniforms with configurable smoothing
- **Professional I/O** — Syphon, NDI, Spout, RTMP, RTSP, webcam, video file
- **bt.709 colorspace** — correct color tagging on all FFmpeg output
- **4K + MSAA** — runtime resolution and anti-aliasing control
- **Graph-based** — composable node topology, deterministic execution

---

## I/O at a glance

| Inputs | Outputs |
|--------|---------|
| Syphon (macOS) | Syphon (macOS) |
| NDI | NDI |
| Webcam | RTMP stream |
| Video file (any ffmpeg format) | RTSP stream |
| RTMP / RTSP / SRT | File recording (H.264) |
| MIDI CC | Preview window |
| OSC | Spout (Windows) |

---

## Templates

Four ready-to-run reference instruments. Clone one and you're rendering on the GPU in minutes.

| Template | Description |
|----------|-------------|
| [`scheng-gradient`](https://github.com/yourorg/scheng-gradient) | Minimal hot-reload shader starter |
| [`scheng-mixer`](https://github.com/yourorg/scheng-mixer) | Two Syphon inputs, MIDI T-bar crossfade |
| [`scheng-processor`](https://github.com/yourorg/scheng-processor) | Webcam input, live shader effect, MIDI control |
| [`scheng-video-mixer`](https://github.com/yourorg/scheng-video-mixer) | Two video files, MIDI T-bar crossfade |

---

## Quick start

```bash
# Clone a template
git clone https://github.com/yourorg/scheng-gradient
cd scheng-gradient

# Build and run
cargo run --release
```

Requirements: Rust 1.75+, ffmpeg in PATH. macOS: `vendor/Syphon.framework` for Syphon I/O.

---

## Documentation

| What | Where |
|-----|------|
| **Website & Overview** | [scheng.dev](https://scheng.dev) |
| **Developer Reference** | [scheng.dev/developer-reference](https://scheng.dev/developer-reference) |
| **Architecture Diagrams** | [scheng.dev/architecture](https://scheng.dev/architecture) |
| **SDK Quick Reference** | [scheng.dev/sdk-reference](https://scheng.dev/sdk-reference) |

---

## Workspace structure

```
crates/
  scheng-graph               # Graph topology and compilation
  scheng-runtime-wgpu        # wgpu render pipeline and executor
  scheng-param-store         # ParamStore, MIDI/OSC routing, smoothing
  scheng-hotreload           # File watcher, shader hot-reload
  scheng-input-{midi,osc,webcam,video,ndi,syphon,rtmp}
  scheng-output-{syphon,ndi,spout,ffmpeg}
examples/
  scheng-instrument          # Full-featured reference instrument
```

---

## Platform support

| Platform | Backend | Status |
|----------|---------|--------|
| macOS (Apple Silicon / Intel) | Metal | ✅ Primary |
| Windows | DX12 | ✅ Supported |
| Linux | Vulkan | ✅ Supported |

---

## License

MIT — see [LICENSE](LICENSE).
 