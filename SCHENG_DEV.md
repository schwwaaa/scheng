# scheng — Development Reference
> Living document. Update as work progresses.
> Last updated: 2026-03-15

---

## Project North Star

**scheng** is a Rust SDK for building GPU-accelerated video synthesis instruments.
It provides a node-graph execution model, explicit shader contracts, and a modular I/O surface.
Instruments built on the SDK ship as cross-platform native applications via Tauri.
The shader layer is intentionally low-level — enabling developers to implement any signal
processing topology, including faithful reproductions of historical analog video synthesis hardware.

**shadecore** = the proven single-binary reference implementation. Features that work there are the target spec for scheng's SDK crates.

---

## The wgpu Backend Decision

### Decision: Dual Backend — Keep glow, Add wgpu

**Don't delete `scheng-runtime-glow`. Add `scheng-runtime-wgpu` alongside it.**

This is not a migration — it's an extension. The architecture already supports it because
`scheng-runtime` is backend-agnostic. The `OutputSink` trait, `Plan`, `FrameCtx`, and
`NodeProps` don't change at all.

---

### Why Keep glow

- **shadecore validated it.** Syphon, Spout, NDI, FFmpeg, MIDI — everything works end-to-end in glow.
- **GLSL shaders run as-is.** Your entire shader library is native GLSL. No translation needed.
- **Linux/Windows still work fine with OpenGL 3.3+.** No deprecation concern there.
- **It's the regression baseline.** Contract tests run against it. Keep it green.

---

### Why Add wgpu

| Problem | glow | wgpu |
|---|---|---|
| macOS long-term | Deprecated (10.14+), translation layer | Metal native |
| Tauri embedding | Separate GL window, hostile | First-class via `wgpu-core` |
| Cross-platform one backend | No (GL quirks per platform) | Yes (Metal/DX12/Vulkan/WebGPU) |
| WebGPU/browser future | No | Yes (same API) |
| Shader language | GLSL only | WGSL + **GLSL via naga** |

The GLSL → wgpu path works via **naga**, wgpu's shader translation library. Naga compiles
GLSL fragment shaders to WGSL/SPIR-V/Metal at load time. This means your existing shader
library stays GLSL — naga handles the translation transparently.

```
your .frag file (GLSL 330 core)
       │
       ▼ naga::front::glsl
  naga IR (intermediate)
       │
       ├──▶ WGSL    (wgpu native / WebGPU)
       ├──▶ MSL     (Metal / macOS / iOS)
       ├──▶ HLSL    (DX12 / Windows)
       └──▶ SPIR-V  (Vulkan / Linux)
```

**Known naga GLSL limitation:** naga's GLSL frontend targets GLSL 450 / Vulkan-style GLSL,
not the exact `#version 330 core` profile used in shadecore. The delta is small:
- `gl_FragCoord` works fine
- `texture()` calls work fine
- Extensions and legacy builtins may need minor adjustment
- The `v_uv` vertex output convention is compatible

Mitigation: write a small compatibility header injected at the top of every shader before
naga compilation. This normalizes any 330→450 gaps transparently.

---

### How wgpu Slots Into the Crate Structure

No existing crate changes. One new crate added:

```
crates/
├── scheng-core              (unchanged)
├── scheng-graph             (unchanged)
├── scheng-runtime           (unchanged — abstract contracts)
├── scheng-runtime-glow      (unchanged — proven baseline)
├── scheng-runtime-wgpu      (NEW — Metal/DX12/Vulkan backend)
│     ├── Depends on: scheng-runtime, scheng-graph, wgpu
│     ├── Implements: execute_plan_to_sink() via wgpu render passes
│     ├── Manages: wgpu Device, Queue, bind groups, render pipelines
│     └── GLSL shaders compiled via naga at load time
├── scheng-bridge            (unchanged)
...
```

**Cargo feature flags on the host binary:**

```toml
[features]
default  = ["backend-wgpu"]
gl       = ["scheng-runtime-glow"]
wgpu     = ["scheng-runtime-wgpu"]  # default going forward
```

---

### Tauri Integration Model

```
Tauri Binary
├── Rust backend
│     ├── scheng-runtime-wgpu running on dedicated render thread
│     ├── wgpu renders into offscreen texture (not visible yet)
│     ├── OutputSink implementations: Syphon, Spout, NDI, FFmpeg
│     └── Tauri commands: set_param, load_graph, set_output_mode
│
└── WebView (WKWebView on macOS, WebView2 on Windows)
      ├── Instrument UI: parameter sliders, graph editor (future)
      ├── Calls Tauri commands via invoke()
      └── Receives preview frames via IPC (optional, downscaled JPEG/PNG)
```

The render loop runs entirely in native Rust. The WebView is a control surface only.
Preview frames can be sent via Tauri's event system as base64-encoded images at 15-30fps —
enough for monitoring without impacting GPU performance.

---

### wgpu Backend Implementation Checklist

- [ ] Add `scheng-runtime-wgpu` crate to workspace
- [ ] Add wgpu, naga as dependencies
- [ ] Implement `RenderTarget` (wgpu Texture + TextureView + Framebuffer equivalent)
- [ ] Implement shader compilation: GLSL → naga → wgpu `ShaderModule`
- [ ] Implement GLSL compat header (330→450 normalization)
- [ ] Implement uniform binding: uTime, uResolution, uFrame, custom u_ params
- [ ] Implement texture binding: iChannel0..iChannel3 via bind groups
- [ ] Implement `execute_plan_to_sink()` — iterate Plan, dispatch render passes
- [ ] Implement ping-pong buffers for PreviousFrame / Feedback nodes
- [ ] Port `OutputSink` implementations: preview window, Syphon, Spout
- [ ] Add contract tests running against wgpu backend (same golden fixtures)
- [ ] Feature-flag the backend selection in the host binary

---

## Master Punchlist

Status key: `[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked

---

### Phase 0 — Orientation & Baseline ✓

- [x] Read and understand scheng codebase + docs
- [x] Read and understand shadecore features
- [x] Map shadecore features → scheng SDK gaps
- [x] Make wgpu backend decision
- [ ] Verify `cargo build --workspace` passes clean on current main
- [ ] Verify `cargo test -p scheng-contract-tests` passes
- [ ] Verify shadecore `cargo run` produces a render window with default shader

---

### Phase 1 — wgpu Backend (current focus)

- [x] Create `crates/scheng-runtime-wgpu/` with Cargo.toml
- [~] Add to workspace Cargo.toml members list  ← YOU ARE HERE: add "crates/scheng-runtime-wgpu" to members
- [x] Scaffold: lib.rs, context.rs, compat.rs, shader.rs, render_target.rs, uniforms.rs, pipeline.rs, executor.rs
- [x] Wire up wgpu Device + Queue initialization (headless) — context.rs
- [x] Implement GLSL compat header for naga — compat.rs (bindings 0-5, split texture/sampler, #define aliases)
- [~] Compile a single hardcoded frag shader through naga → wgpu pipeline  ← verify with: cargo test test_naga_glsl_compile
- [~] Render one frame to offscreen texture — verify pixel readback  ← cargo test test_single_node_renders
- [x] Implement execute_frame() with full Plan iteration — executor.rs (ShaderSource → PixelsOut)
- [x] Implement iChannel0..3 via bind groups (split texture+sampler, blank fallback)
- [x] Implement FrameBlock uniform buffer (uTime, uResolution, uFrame) — uniforms.rs
- [ ] Implement custom u_ uniform injection — Phase 1.2
- [ ] Port PingPongTarget for Feedback/PreviousFrame nodes — Phase 1.3
- [~] Run integration tests against wgpu backend  ← cargo test -p scheng-runtime-wgpu -- --nocapture
- [~] CLI test: single shader, pixel readback  ← cargo test test_custom_frag_shader

---

### Phase 2 — Input Layer (port from shadecore)

- [ ] `scheng-input-video`: audit current state, verify gapless decode → GL texture
- [ ] `scheng-input-video`: add wgpu texture upload path
- [ ] `scheng-input-webcam`: verify native feature builds on macOS + Windows
- [ ] `scheng-input-webcam`: wgpu upload path
- [ ] `scheng-input-midi`: new crate — port from shadecore's midir integration
  - [ ] CoreMIDI on macOS
  - [ ] cross-platform via midir
  - [ ] JSON mapping: CC → NodeId + param name (matches shadecore params.json model)
- [ ] `scheng-input-ndi` (receive side): NDI frames → texture
- [ ] `scheng-input-syphon` (macOS receive): Syphon client → texture
- [ ] `scheng-input-spout` (Windows receive): Spout receiver → texture

---

### Phase 3 — Output Layer (port from shadecore)

- [ ] `scheng-output-syphon`: port syphon_bridge.m from shadecore, wire OutputSink
- [ ] `scheng-output-spout`: port C++ Spout2 bridge from shadecore, wire OutputSink
- [ ] `scheng-output-ffmpeg`: port recording.rs FFmpeg worker from shadecore
  - [ ] Bounded queue model (drop frames rather than stall render)
  - [ ] RTSP / RTMP streaming mode
  - [ ] Local file recording mode
  - [ ] JSON config: codec, bitrate, preset, framerate, resolution
- [ ] `scheng-output-ndi`: formalize NDI sender as OutputSink
- [ ] Headless / offscreen OutputSink (pixel readback only — for testing + CLI tools)

---

### Phase 4 — Control Layer

- [ ] Consolidate `scheng-control-osc` (exists, verify it works)
- [ ] Port MIDI from shadecore into `scheng-input-midi`
- [ ] `scrubbable_controls`: verify keymap.json / osc_map.json hot-reload
- [ ] State ownership model: match shadecore's authority table
  - render.json → active shader
  - params.json → parameter defaults + MIDI map
  - output.json → output mode + hotkeys
  - recording.json → recording profiles
- [ ] Hot-reload: file watcher → reload shader source without restart (port hotreload.rs)

---

### Phase 5 — Tauri Shell

- [ ] Create `crates/scheng-tauri` — the Tauri integration crate
- [ ] Embed scheng-runtime-wgpu in Tauri backend
- [ ] Define IPC command surface:
  - `set_param(node_id, param_name, value: f32)`
  - `load_graph(graph_json: String)`
  - `set_output_mode(mode: String)`
  - `get_params() -> NodePropsSnapshot`
  - `start_recording() / stop_recording()`
- [ ] Preview frame IPC: readback → JPEG → Tauri event at 15fps
- [ ] Create minimal instrument template (fork-ready)
- [ ] Test: macOS `.app` build via `tauri build`
- [ ] Test: Windows `.exe` build via `tauri build`
- [ ] Test: Linux build via `tauri build`

---

### Phase 6 — Shader Library (LZX-inspired)

> Goal: reverse-engineer historical analog video synthesis hardware as GLSL shaders.
> Each shader = a documented module with its own params.json profile.

- [ ] Colorizer (hue rotation, RGB→component, component→RGB)
- [ ] Ramp generator (horizontal, vertical, radial, angular)
- [ ] Key generator (luma key, chroma key, threshold)
- [ ] Hard keyer
- [ ] Soft keyer
- [ ] Fade to black / white
- [ ] Proc amp (brightness, contrast, saturation, hue)
- [ ] Sync processor (H-sync, V-sync simulation)
- [ ] Video feedback (ping-pong, convergence modes)
- [ ] Matrix mixer 4→1 (weighted sum)
- [ ] Crossfader (T-bar, soft cuts)
- [ ] Waveform monitor (parade, overlay)
- [ ] Vectorscope
- [ ] Pattern generator (color bars, test card, grid)

---

### Phase 7 — GUI / Editor (deferred — design carefully)

> Don't build until the SDK surface is stable and fully tested CLI-first.

- [ ] Research: node graph editor options (egui, iced, or Tauri-based)
- [ ] Define the prototyping → export workflow
- [ ] Define the JSON graph format for save/load
- [ ] Design the parameter UI contract (sliders, toggles, XY pads)
- [ ] Design the preview display model in GUI context

---

## CLI Testing Guide

### Prerequisites

```bash
# Install Rust stable
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# macOS: Xcode CLI tools
xcode-select --install

# macOS + Syphon output: place Syphon.framework at vendor/Syphon.framework
# Windows + Spout: C++ build tools required
```

### 1. Build everything

```bash
cd scheng
cargo build --workspace
```

Expected: all crates compile, no errors.

### 2. Run contract tests

```bash
cargo test -p scheng-contract-tests
```

Expected: all golden fixture tests pass. These pin the public SDK surface.
If these fail after any change, the SDK has a regression.

### 3. Run all tests

```bash
cargo test --workspace
```

### 4. Run the bridge (engine + browser editor)

```bash
cargo run -p scheng-bridge

# With debug logging:
RUST_LOG=scheng_bridge=debug cargo run -p scheng-bridge

# Custom WebSocket address:
SCHENG_BRIDGE_ADDR=0.0.0.0:7777 cargo run -p scheng-bridge
```

Then open `crates/scheng-bridge/scheng-editor.html` in Chrome/Firefox → Connect → load a template → Compile.

### 5. Test OSC control

```bash
# Start bridge in one terminal
cargo run -p scheng-bridge

# Send OSC from another terminal (requires oscsend or similar)
oscsend 127.0.0.1 9000 /scheng/node/xfad/uniform/u_tbar f 0.75
oscsend 127.0.0.1 9000 /scheng/node/key/uniform/u_thresh f 0.35
```

### 6. Test contract tests with verbose output

```bash
cargo test -p scheng-contract-tests -- --nocapture
```

### 7. Validate sdk-compat (API regression check)

```bash
cargo build -p sdk-compat
```

This is a compile-only witness. If it doesn't compile, the public API has broken.

### 8. Build with Syphon (macOS only)

```bash
cargo build -p scheng-runtime-glow --features syphon
```

### 9. Build with webcam input

```bash
cargo build -p scheng-input-webcam --features native
```

---

## shadecore Reference Commands

> shadecore is the proven single-binary reference. Use these to verify features work
> before porting to scheng.

```bash
cd shadecore

# Standard run
cargo run

# With NDI output
cargo run --features ndi

# Observe render hotkeys at runtime:
# 1 = preview only
# 2 = Syphon (macOS)
# 3 = Spout (Windows)
# 4 = FFmpeg stream
# 6 = NDI
```

---

## Key Architecture Rules (Never Violate)

1. **The engine does not own time.** FrameCtx is always supplied by the host.
2. **Topology is static after compile().** NodeProps is dynamic. Structural changes require recompile.
3. **No layer reaches backward.** Graph → Runtime → Backend. Never reverse.
4. **Control updates NodeProps only.** OSC/MIDI never touches the graph or render loop directly.
5. **The runtime is single-threaded.** GL/wgpu context must be on the calling thread.
6. **Errors are returned immediately.** No silent recovery anywhere in the engine.
7. **Optional features are truly optional.** Syphon, Spout, webcam never bleed into core crates.

---

## GLSL Shader Contract (v_uv convention)

All fragment shaders must follow this interface:

```glsl
#version 330 core
in vec2 v_uv;               // lowercase — matches vertex shader out
out vec4 fragColor;

uniform sampler2D iChannel0; // input textures, up to iChannel3
uniform float uTime;         // seconds since start
uniform vec2 uResolution;    // output dimensions
uniform int uFrame;          // monotonic frame counter

// Custom params — u_ prefix by convention
uniform float u_myParam;
```

Port mappings → texture units:
| Port name | iChannel |
|---|---|
| `in`, `in0`, `a`, `src` | iChannel0 |
| `in1`, `b`, `src1` | iChannel1 |
| `in2`, `c`, `src2` | iChannel2 |
| `in3`, `d`, `src3` | iChannel3 |

---

## Dependency Graph (current)

```
scheng-editor.html
        │ WebSocket JSON
        ▼
scheng-bridge  (tokio + tungstenite)
  graph_manager.rs
        │
        ├── scheng-runtime-glow  (OpenGL / glow)  ← proven baseline
        │     ├── scheng-runtime  (abstract ops, params, banks)
        │     │     └── scheng-graph  (node/port/edge/plan)
        │     │           └── scheng-core  (error, config, events)
        │     └── scheng-input-video
        │
        └── [scheng-runtime-wgpu]  ← to be added (Metal/DX12/Vulkan)

scheng-passes       (ping-pong, temporal ring)
scheng-buffers      (GPU ring buffer)
scheng-host-winit   (window + GL context)
scheng-input-webcam (camera, optional)
scheng-control-osc  (UDP OSC)
scrubbable_controls (keyboard + OSC layer)
scheng-contract-tests
sdk-compat
```

---

## Open Questions (resolve before building)

- [ ] **naga GLSL compatibility**: Run a shadecore shader through naga today. Document any
       adjustments needed for the compat header.
- [ ] **wgpu + winit on macOS**: Verify wgpu + winit event loop works without the OpenGL
       path. The winit version in scheng-host-winit may need updating.
- [ ] **Tauri + wgpu surface sharing**: Can wgpu render into a surface that Tauri's WebView
       can composite over, or does the preview need to go through IPC frames?
       Research: `wgpu` + `raw-window-handle` + Tauri's `setup` hook.
- [ ] **shadecore Vosk resources**: Speech recognition library present in repo.
       Intentional? If yes, add `scheng-input-voice` to Phase 2.
- [ ] **Max/MSP integration**: `max/` directory in scheng repo. Define the IPC boundary.

---

## Phase 1 Integration Notes

### Files Created in This Session

```
crates/scheng-runtime-wgpu/
├── Cargo.toml            ← dependencies: wgpu=22, naga=22, bytemuck, pollster, regex
├── src/
│   ├── lib.rs            ← public API, re-exports, WgpuError enum
│   ├── context.rs        ← WgpuContext::new() — headless Device+Queue init
│   ├── compat.rs         ← GLSL 330→450 preprocessor (strip, rewrite, inject header)
│   ├── shader.rs         ← ShaderCache: GLSL→naga→wgpu::ShaderModule
│   ├── render_target.rs  ← RenderTarget: offscreen RGBA8 texture + readback
│   ├── uniforms.rs       ← UniformManager: FrameBlock buffer (uTime/uResolution/uFrame)
│   ├── pipeline.rs       ← PipelineCache: RenderPipeline per shader hash
│   └── executor.rs       ← WgpuRuntime::execute_frame(), OutputSink trait, NodeConfig
└── tests/
    └── headless.rs       ← 8 integration tests (compat, naga, GPU rendering, readback)
```

### Step 1: Add to Workspace

In the root `Cargo.toml`, add to the `[workspace]` members list:
```toml
[workspace]
members = [
    "crates/scheng-core",
    "crates/scheng-graph",
    "crates/scheng-runtime",
    "crates/scheng-runtime-glow",
    "crates/scheng-runtime-wgpu",   # ← ADD THIS
    # ... other crates
]
```

### Step 2: Verify Plan API (CRITICAL — do this before running tests)

The executor uses `plan.node_ids()`, `plan.node_kind(id)`, and `plan.inputs_for(id)`.
These method names are assumed from the README. Check the actual scheng-graph source:

```bash
grep -n "pub fn" crates/scheng-graph/src/lib.rs
```

Adjust `executor.rs` to match. The three key calls:
- Iterating plan nodes in order
- Getting a node's `NodeKind`
- Getting the upstream `(NodeId, channel_index)` pairs for a node's inputs

### Step 3: Run the CPU-only tests first (no GPU needed)

```bash
cargo test -p scheng-runtime-wgpu test_compat_preprocessing -- --nocapture
cargo test -p scheng-runtime-wgpu test_naga_glsl_compile -- --nocapture
```

These must pass before any GPU tests. If `test_naga_glsl_compile` fails:
- The naga GLSL 450 source in compat.rs has a syntax error
- Check the processed source printed by `--nocapture` for the exact error line

### Step 4: Run GPU tests

```bash
cargo test -p scheng-runtime-wgpu -- --nocapture
```

If no adapter is found in CI: `WGPU_BACKEND=gl cargo test -p scheng-runtime-wgpu`

### Step 5: Fix `once_cell` dependency

`compat.rs` uses `once_cell::sync::Lazy` for compiled regexes.
Add to Cargo.toml if not already present in workspace:
```toml
once_cell = "1"
```
Or replace with `std::sync::OnceLock` (Rust 1.70+):
```rust
use std::sync::OnceLock;
static RE_VERSION: OnceLock<Regex> = OnceLock::new();
// in the function:
RE_VERSION.get_or_init(|| Regex::new(...).unwrap())
```

### Known Phase 1 Gaps (to fix in 1.2/1.3)

| Gap | Location | Phase |
|---|---|---|
| Custom u_ uniforms stripped with warning | compat.rs, executor.rs | 1.2 |
| plan.inputs_for() API needs verification | executor.rs line ~140 | NOW |
| plan.node_kind() API needs verification | executor.rs line ~105 | NOW |
| Y-axis flip vs OpenGL convention | render_target.rs, shader.rs | 1.5 |
| Vertex module created per-pipeline (wasteful) | pipeline.rs | 1.2 |
| PingPong / Feedback node support | executor.rs | 1.3 |
| Hot-reload: recompile on source change | executor.rs | Phase 4 |
| scheng-runtime NodeProps integration | executor.rs NodeConfig | After API verified |
