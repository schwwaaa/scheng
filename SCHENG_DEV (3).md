# scheng — Development Reference
> Last updated: 2026-03-15 — Phases 1–4 complete

---

## Project North Star

scheng is a Rust SDK for building GPU-accelerated video synthesis instruments.
Node-graph execution model, explicit shader contracts, modular I/O (Syphon, Spout,
NDI, FFmpeg). Instruments ship cross-platform via Tauri. Shader layer is intentionally
low-level — enabling faithful reproductions of historical analog video synthesis hardware.

shadecore = proven single-binary reference. Features that work there are the target spec.

---

## Workspace Layout (current state)

```
crates/
├── scheng-core                  Foundation: config, assets, error, events
├── scheng-graph                 Node/port/edge/plan model ✓
├── scheng-runtime               Backend-agnostic ops, params, banks ✓
├── scheng-runtime-glow          OpenGL backend (proven in shadecore) ✓
├── scheng-runtime-wgpu          Metal/DX12/Vulkan backend ✅ PHASE 1
├── scheng-bridge                WebSocket bridge + browser editor ✓
├── scheng-passes                Ping-pong, temporal ring buffers
├── scheng-buffers               GPU ring buffer primitives
├── scheng-host-winit            winit + glutin window/context
├── scheng-input-video           Video file → GL texture
├── scheng-input-webcam          Webcam → RGBA (optional)
├── scheng-control-osc           UDP OSC receiver (existing, rosc-based)
├── scheng-contract-tests        Golden fixture + behavioral contract tests ✓
├── scrubbable_controls          Keyboard + OSC control layer (existing)
├── sdk-compat                   Compile-only API regression witness ✓
│
│   ── Phase 3: Output crates ──
├── scheng-output-ffmpeg         FFmpeg: RTSP/RTMP/file recording ✅ PHASE 3
├── scheng-output-syphon         Syphon Metal (macOS) ✅ PHASE 3
├── scheng-output-spout          Spout2 (Windows) — bridge port needed
├── scheng-output-ndi            NDI — SDK stub, interface defined
│
│   ── Phase 4: Control crates ──
├── scheng-param-store           JSON schema → live values → NodeConfig ✅ PHASE 4
├── scheng-input-midi            MIDI CC → ParamStore (midir) ✅ PHASE 4
├── scheng-control-osc-wgpu      OSC UDP → ParamStore adapter ✅ PHASE 4
└── scheng-hotreload             File watcher → shader/params reload ✅ PHASE 4
```

---

## Master Punchlist

Status: `[x]` done · `[~]` in progress · `[ ]` todo · `[!]` blocked

### Phase 0 — Baseline
- [x] Read and understand scheng + shadecore codebases
- [x] Map shadecore features → scheng SDK gaps
- [x] wgpu backend decision
- [ ] Verify `cargo build --workspace` passes clean on current main
- [ ] Verify `cargo test -p scheng-contract-tests` passes

### Phase 1 — wgpu Backend ✅ COMPLETE (11/11 tests on Apple M1 Max)
- [x] scheng-runtime-wgpu: WgpuContext (Metal/DX12/Vulkan headless init)
- [x] compat.rs: GLSL 330→450 preprocessor (bindings 0-5, split iChannelN, #define aliases)
- [x] shader.rs: GLSL→naga→wgpu ShaderModule cache
- [x] render_target.rs: offscreen RGBA8 + CPU readback
- [x] uniforms.rs: FrameBlock uniform buffer (uTime/uResolution/uFrame, 16 bytes)
- [x] pipeline.rs: RenderPipeline cache + bind group layout
- [x] executor.rs: execute_frame(), OutputSink trait, PixelReadbackSink
- [x] 11/11 tests passing on Apple M1 Max (Metal backend)
- [ ] Custom u_* uniform injection — Phase 1.2
- [ ] PingPong buffers for Feedback/PreviousFrame — Phase 1.3

### Phase 2 — Input Layer
- [ ] scheng-input-video: wgpu texture upload path
- [ ] scheng-input-webcam: wgpu upload path
- [ ] scheng-input-ndi (receive side)
- [ ] scheng-input-syphon (macOS receive)
- [ ] scheng-input-spout (Windows receive)

### Phase 3 — Output Layer ✅ COMPLETE
- [x] scheng-output-ffmpeg: FfmpegSink, FfmpegWorker, FfmpegConfig (RTSP/RTMP/File)
- [x] scheng-output-syphon: SyphonSink, Metal ObjC bridge, build.rs
- [~] scheng-output-spout: interface + FFI ready — copy native bridge from scheng-runtime-glow
- [~] scheng-output-ndi: NdiSink + NdiConfig ready — wire NDI SDK Rust bindings
- [x] Headless OutputSink: PixelReadbackSink in scheng-runtime-wgpu

### Phase 4 — Control Layer ✅ COMPLETE
- [x] scheng-param-store: ParamSchema (params.json), ParamStore (targets+smoothing), NodeConfigBuilder
- [x] scheng-input-midi: MidiInput (midir, CoreMIDI/WinMM/ALSA), CC→ParamStore
- [x] scheng-control-osc-wgpu: OscReceiver (non-blocking UDP poll), OSC→ParamStore
- [x] scheng-hotreload: AssetWatcher (notify FSEvents/inotify), HotReloader (shader + params.json)
- [x] CONTROL_LAYER_README.md: complete instrument wiring example

### Phase 5 — Tauri Shell ✅ COMPLETE
- [x] Create crates/scheng-tauri
- [x] Embed scheng-runtime-wgpu in Tauri Rust backend on dedicated thread
- [x] IPC surface: set_param, load_graph, set_output_mode, start/stop_recording, get_engine_status
- [x] Preview frame IPC: readback → JPEG (scaled 320×180) → Tauri event ~15fps
- [x] Instrument template: ui/index.html (param sliders, preview, output mode, recording)
- [x] macOS: cargo tauri dev confirmed running on Apple M1 Max


### First-run: create assets/

The render thread looks for `assets/` relative to CWD.
Create these two files to get live preview working:

```bash
mkdir -p assets/shaders

# assets/params.json
cat > assets/params.json << 'JSON'
{"version":1,"params":[{"name":"u_speed","min":0,"max":5,"default":1,"smooth":0.05,"midi_cc":1}]}
JSON

# assets/shaders/default.frag
cat > assets/shaders/default.frag << 'GLSL'
#version 330 core
in vec2 v_uv;
out vec4 fragColor;
uniform float uTime;
void main() {
    float r = v_uv.x + 0.5 * sin(uTime);
    float g = v_uv.y + 0.5 * cos(uTime * 0.7);
    fragColor = vec4(r, g, 0.2, 1.0);
}
GLSL
```

The render thread hot-reloads both files — edit and save while running.

### Phase 6 — Shader Library (LZX-inspired)
- [ ] Colorizer (hue rotation, RGB↔component)
- [ ] Ramp generator (H, V, radial, angular)
- [ ] Luma keyer / chroma keyer
- [ ] Hard keyer / soft keyer
- [ ] Proc amp (brightness, contrast, saturation, hue)
- [ ] Crossfader with T-bar uniform
- [ ] Matrix mixer 4→1
- [ ] Video feedback (ping-pong convergence)
- [ ] Pattern generator (color bars, test card, grid)
- [ ] Waveform monitor / vectorscope

### Phase 7 — GUI / Editor (design before building)
- [ ] Node graph editor research
- [ ] Prototyping → export workflow definition
- [ ] JSON graph save/load format
- [ ] Parameter UI contract

---

## CLI Test Reference

```bash
# Phase 1 — wgpu backend (all 11 tests)
cargo test -p scheng-runtime-wgpu -- --nocapture

# Phase 1 — CPU-only subset (no GPU needed)
cargo test -p scheng-runtime-wgpu compat         -- --nocapture
cargo test -p scheng-runtime-wgpu test_naga      -- --nocapture
cargo test -p scheng-runtime-wgpu size_is_16     -- --nocapture
cargo test -p scheng-runtime-wgpu align_to_256   -- --nocapture

# Phase 3 — output crate checks
cargo check -p scheng-output-ffmpeg
cargo check -p scheng-output-syphon
cargo check -p scheng-output-spout
cargo check -p scheng-output-ndi

# Phase 3 — FFmpeg config + arg builder tests (no ffmpeg needed)
cargo test -p scheng-output-ffmpeg -- --nocapture

# Phase 4 — control layer
cargo test -p scheng-param-store   -- --nocapture
cargo test -p scheng-input-midi    -- --nocapture
cargo test -p scheng-control-osc-wgpu -- --nocapture

# Contract tests (scheng-graph public API regression)
cargo test -p scheng-contract-tests -- --nocapture

# SDK compat witness (compile-only)
cargo build -p sdk-compat

# Full workspace
cargo build --workspace
cargo test --workspace
```

---

## GLSL Shader Contract (verified working on Metal)

```glsl
#version 330 core          // stripped by compat.rs
in vec2 v_uv;              // stripped, provided by compat header
out vec4 fragColor;        // stripped, provided by compat header
uniform sampler2D iChannel0; // up to iChannel3 — stripped, split texture+sampler
uniform float uTime;       // stripped, in FrameBlock
uniform vec2 uResolution;  // stripped, in FrameBlock

void main() {
    vec2 uv = v_uv;
    fragColor = texture(iChannel0, uv) + vec4(sin(uTime) * 0.1);
}
```

## Bind Group Layout (bindings 0–5)

| binding | resource | GLSL alias |
|--------:|---------|------------|
| 0 | texture2D | iChannel0 |
| 1 | texture2D | iChannel1 |
| 2 | texture2D | iChannel2 |
| 3 | texture2D | iChannel3 |
| 4 | sampler(filter) | iSampler |
| 5 | UniformBuffer | FrameBlock (uResolution, uTime, uFrame) |

---

## Architecture Rules (never violate)

1. Engine does not own time — FrameCtx always supplied by host
2. Topology static after compile() — NodeConfig dynamic
3. No layer reaches backward — Graph → Runtime → Backend
4. Control writes NodeConfig/ParamStore only — never touches graph or render loop
5. Runtime is single-threaded — GPU context on calling thread only
6. Errors returned immediately — no silent recovery
7. Optional features truly optional — Syphon/Spout/webcam never in core crates
8. submit() before present() — sink.present() always after queue.submit()

---

## Known Phase 1 Gaps (Phase 1.2/1.3)

| Gap | Phase | Location |
|-----|-------|----------|
| Custom u_* uniform injection | 1.2 | executor.rs + NodeConfig + compat.rs |
| PingPong buffers (Feedback/PreviousFrame) | 1.3 | new: scheng-passes-wgpu |
| Y-axis flip vs OpenGL (top-left vs bottom-left) | 1.5 | presenter layer |
| scheng-runtime NodeProps integration | after API verified | executor.rs NodeConfig |

---

## Dependency Map (as-built)

```
scheng-editor.html ──WebSocket──▶ scheng-bridge
                                        │
                              scheng-runtime-glow  (OpenGL, proven)
                              scheng-runtime-wgpu  (Metal/DX12/Vulkan) ✅
                                   │  │
                         scheng-runtime (abstract)
                              │
                         scheng-graph
                              │
                         scheng-core

scheng-output-ffmpeg  ──▶ scheng-runtime-wgpu OutputSink ✅
scheng-output-syphon  ──▶ scheng-runtime-wgpu OutputSink ✅
scheng-output-spout   ──▶ scheng-runtime-wgpu OutputSink (~)
scheng-output-ndi     ──▶ scheng-runtime-wgpu OutputSink (~)

scheng-param-store    ──▶ scheng-graph, scheng-runtime-wgpu ✅
scheng-input-midi     ──▶ scheng-param-store (Arc<Mutex<>>) ✅
scheng-control-osc-wgpu ──▶ scheng-param-store ✅
scheng-hotreload      ──▶ scheng-param-store, scheng-runtime-wgpu ✅
```
