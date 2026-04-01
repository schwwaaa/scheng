# Changelog

All notable changes to scheng are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/). Versioning follows [Semantic Versioning](https://semver.org/).

---

## [Unreleased]

---

## [0.1.0] — 2026-03-25

Initial SDK release. All core I/O primitives, four reference templates, full documentation.

### Added

**Core engine**
- `scheng-runtime-wgpu` — wgpu Metal/DX12/Vulkan render pipeline with `execute_frame()`
- Rgba16Float internal render targets (16-bit precision, no banding across passes)
- Graph-based node system with topological sort and multi-input routing
- Custom shader uniforms (`u_*`) packed into `CustomBlock` GPU buffer at binding 6 (Phase 1.2)
- Pipeline cache keyed on `(shader_hash, sample_count)` — lazy compile, zero recompile overhead per frame
- MSAA anti-aliasing (`sample_count` in `FrameCtx` — 1x, 4x, 8x)
- 4K resolution support via runtime `FrameCtx` parameters
- bt.709 colorspace tags on all FFmpeg output
- `NodeConfig.uniforms` HashMap wired end-to-end from MIDI → ParamStore → shader

**Inputs**
- `scheng-input-midi` — midir wrapper, CC routing to ParamStore
- `scheng-control-osc-wgpu` — UDP OSC receiver, address routing
- `scheng-input-webcam` — nokhwa, MJPEG/YUYV fallback, GPU texture upload
- `scheng-input-video` — ffmpeg-next 8, BICUBIC scaling, looping, `texture_arc()`
- `scheng-input-ndi` — grafton-ndi 0.11, NDI source discovery and receive
- `scheng-input-syphon` — ObjC Metal bridge, persistent `SyphonServerDirectory`, deferred discovery, BGRA→RGBA swap
- `scheng-input-rtmp` — ffmpeg subprocess, RTMP/RTSP/SRT/HLS receive, bounded channel

**Outputs**
- `scheng-output-syphon` — Metal texture sharing, zero-copy on Apple Silicon
- `scheng-output-ndi` — NewTek SDK, NDI frame push
- `scheng-output-spout` — Windows stub (C++ bridge required — see roadmap)
- `scheng-output-ffmpeg` — RTMP stream, RTSP stream, H.264 file recording, bt.709 tags, non-blocking

**Infrastructure**
- `scheng-param-store` — ParamStore, ParamSchema, per-frame smoothing, MIDI CC index, OSC address index
- `scheng-hotreload` — AssetWatcher, file change events
- rpath embed in `build.rs` — no `DYLD_FRAMEWORK_PATH` required at runtime
- Zero warnings across workspace

**Templates**
- `scheng-gradient` — minimal hot-reload shader starter
- `scheng-mixer` — two Syphon inputs, MIDI T-bar crossfade, Syphon output
- `scheng-processor` — webcam input, solarize effect, MIDI CC1–CC4, Syphon output
- `scheng-video-mixer` — two video files, MIDI T-bar crossfade, Syphon output

**Documentation**
- User-facing website (`index.html`, `architecture.html`)
- Developer Reference — Docker/Twilio-style comprehensive reference (`developer-reference.html`)
- SDK Quick Reference (`sdk-reference.html`)
- Architecture SVG diagrams — full system, frame loop, graph topology, MIDI routing, use case flows
- Technical SDK reference (`.docx`)
- `CONTRIBUTING.md`, `CHANGELOG.md`, `LICENSE`

### Fixed
- `executor.rs` — output nodes presented after `queue.submit()` (fixed all-zero pixel readback)
- `scheng-input-syphon` — BGRA→RGBA channel swap in ObjC Metal bridge
- `scheng-input-syphon` — deferred directory discovery to frame 5+ prevents empty source list
- `scheng-input-webcam` — Y-flip in passthrough shader matches GPU UV space
- Blit shader — uses interpolated vertex UVs (not `textureDimensions()`) for correct window resize behavior
- `test_previous_frame_node` — relaxed assertion for gradient on frame 0
- `VideoTexture` — wraps `wgpu::Texture` in `Arc` enabling `texture_arc()` on `VideoDecoder`

---

## Roadmap

### Near-term
- **Plugin ecosystem** — `InputSource` and `OutputSink` traits published, plugin contract spec finalized
- **`scheng-playground`** — interactive multi-shader explorer with keyboard shader switching
- **SDF template** — signed distance field / vector graphics demonstration
- **Raymarching template** — 3D via raymarched scenes in fragment shaders, no vertex pipeline required
- **Spout output (Windows)** — C++ DX texture sharing bridge

### Medium-term
- **SBC / embedded targets** — Raspberry Pi 4/5 (wgpu Vulkan via Mesa), NVIDIA Jetson (wgpu Vulkan + NDI ARM)
- **3D vertex pipeline** — extend WgpuRuntime to support mesh-based rendering alongside fullscreen quad
- **wgpu 24 upgrade** — post-stabilization
- **Frame interpolation** — RIFE or optical flow between decoded video frames

### Long-term
- **sRGB/linear colorspace** — correct gamma handling for webcam preview
- **University / education toolkit** — simplified API surface, project templates for creative coding courses

---

[Unreleased]: https://github.com/yourusername/scheng/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/yourusername/scheng/releases/tag/v0.1.0
