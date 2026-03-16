# scheng — Development Reference
> Last updated: 2026-03-15 — Phase 1 complete

---

## Project North Star

scheng is a Rust SDK for building GPU-accelerated video synthesis instruments.
Node-graph execution model, explicit shader contracts, modular I/O (Syphon, Spout,
NDI, FFmpeg). Instruments ship cross-platform via Tauri. Shader layer is intentionally
low-level — enabling faithful reproductions of historical analog video synthesis hardware.

shadecore = proven single-binary reference. Features that work there are the target spec.

---

## Phase 1 — wgpu Backend ✅ COMPLETE

**11/11 tests passing on Apple M1 Max (Metal backend)**

```
Running unittests src/lib.rs
  test uniforms::tests::from_ctx_correct            ... ok
  test uniforms::tests::size_is_16_bytes            ... ok
  test render_target::tests::align_to_256           ... ok
  test compat::tests::custom_uniforms_reported      ... ok
  test compat::tests::strips_version_and_declarations ... ok

Running tests/headless.rs
  test test_naga_compile_cpu_only    ... ok  [CPU only]
  test test_context_init             ... ok  Apple M1 Max
  test test_single_node_renders      ... ok  first px [109, 255, 51, 255]
  test test_two_node_pipeline        ... ok
  test test_time_varies_between_frames ... ok
  test test_custom_shader_solid_red  ... ok  pixel-perfect red confirmed
```

### What was built

```
crates/scheng-runtime-wgpu/
├── Cargo.toml            wgpu=22, naga=22 (glsl-in only), bytemuck, pollster, once_cell, regex
├── src/
│   ├── lib.rs            Public API, WgpuError enum
│   ├── frame_ctx.rs      FrameCtx defined locally (not from scheng-core)
│   ├── context.rs        WgpuContext::new() — headless Metal/DX12/Vulkan Device+Queue
│   ├── compat.rs         GLSL 330→450 preprocessor (strip decls, split iChannelN, inject header)
│   ├── shader.rs         ShaderCache: GLSL→naga→wgpu::ShaderModule, WGSL vertex shader
│   ├── render_target.rs  RenderTarget: offscreen RGBA8 texture + CPU readback
│   ├── uniforms.rs       UniformManager: FrameBlock buffer (uTime/uResolution/uFrame, 16 bytes)
│   ├── pipeline.rs       PipelineCache: RenderPipeline per shader hash + bind group layout
│   └── executor.rs       WgpuRuntime::execute_frame(), OutputSink trait, PixelReadbackSink
└── tests/
    └── headless.rs       6 GPU integration tests (skip gracefully if no adapter)
```

### Key lessons learned (bugs fixed)

| Bug | Root cause | Fix |
|-----|-----------|-----|
| `naga validate` feature missing | Feature doesn't exist in naga 22 | Removed from Cargo.toml |
| `scheng_core::FrameCtx` not found | FrameCtx lives in scheng-runtime-glow | Define locally in frame_ctx.rs |
| `plan.node_ids()` not found | Plan has `pub nodes: Vec<NodeId>` field | Use `plan.nodes.iter()` |
| `NodeKind` not Copy | Clone needed | Take `&NodeKind` throughout |
| Borrow conflict (mut+immut on render_targets) | entry().or_insert_with() held mut borrow | Split into Phase A (resize) + Phase B (encode) |
| Borrow conflict (pipeline + resolve_inputs) | get_or_create() held mut borrow on self | Free functions taking individual fields |
| `errors.iter()` | naga ParseErrors has `.errors` field | `errors.errors.iter()` |
| `@interpolate` mismatch | WGSL default is `perspective,center`; naga emits `perspective` | Add `@interpolate(perspective)` to vertex out |
| All pixels zero | sink.present() called before queue.submit() | Collect outputs, submit first, then present |

### GLSL shader contract (verified working)

```glsl
#version 330 core          // stripped by compat.rs, replaced with 450
in vec2 v_uv;              // stripped, provided by compat header
out vec4 fragColor;        // stripped, provided by compat header
uniform sampler2D iChannel0; // stripped, replaced with split texture+sampler
uniform float uTime;       // stripped, in FrameBlock
uniform vec2 uResolution;  // stripped, in FrameBlock

void main() {
    fragColor = texture(iChannel0, v_uv) + vec4(uTime * 0.1, 0.0, 0.0, 0.0);
}
```

### Bind group layout (fixed, matches compat header)

| binding | resource | GLSL alias |
|--------:|---------|------------|
| 0 | texture2D | iChannel0_tex → iChannel0 macro |
| 1 | texture2D | iChannel1_tex → iChannel1 macro |
| 2 | texture2D | iChannel2_tex → iChannel2 macro |
| 3 | texture2D | iChannel3_tex → iChannel3 macro |
| 4 | sampler(filter) | iSampler |
| 5 | UniformBuffer | FrameBlock (uResolution, uTime, uFrame) |

---

## Master Punchlist

Status: `[x]` done · `[~]` in progress · `[ ]` todo · `[!]` blocked

### Phase 0 — Baseline
- [x] Read and understand scheng + shadecore codebases
- [x] Map shadecore features → scheng SDK gaps
- [x] Make wgpu backend decision
- [ ] Verify `cargo build --workspace` passes clean
- [ ] Verify `cargo test -p scheng-contract-tests` passes

### Phase 1 — wgpu Backend ✅
- [x] Create crates/scheng-runtime-wgpu with Cargo.toml
- [x] Add to workspace Cargo.toml members
- [x] WgpuContext: headless Device+Queue init (Metal/DX12/Vulkan)
- [x] compat.rs: GLSL 330→450 preprocessor
- [x] shader.rs: GLSL→naga→wgpu ShaderModule cache
- [x] render_target.rs: offscreen RGBA8 + CPU readback
- [x] uniforms.rs: FrameBlock uniform buffer
- [x] pipeline.rs: RenderPipeline cache + bind group layout
- [x] executor.rs: execute_frame(), OutputSink trait, PixelReadbackSink
- [x] 11/11 tests passing on Apple M1 Max
- [ ] Custom u_* uniform injection (Phase 1.2)
- [ ] PingPong buffers for Feedback/PreviousFrame nodes (Phase 1.3)

### Phase 2 — Input Layer
- [ ] scheng-input-video: add wgpu texture upload path
- [ ] scheng-input-webcam: wgpu upload path
- [ ] scheng-input-midi: new crate — port from shadecore's midir integration
  - [ ] CoreMIDI / midir cross-platform
  - [ ] JSON CC → NodeId + param name mapping
- [ ] scheng-input-ndi: NDI frames → texture
- [ ] scheng-input-syphon (receive side)
- [ ] scheng-input-spout (receive side)

### Phase 3 — Output Layer
- [ ] scheng-output-syphon: port syphon_bridge.m, wire OutputSink
- [ ] scheng-output-spout: port C++ Spout2 bridge, wire OutputSink
- [ ] scheng-output-ffmpeg: bounded queue worker, RTSP/RTMP + local recording
- [ ] scheng-output-ndi: formalize NDI sender as OutputSink
- [ ] Headless / offscreen OutputSink (pixels only, no window)

### Phase 4 — Control Layer
- [ ] Verify scheng-control-osc end-to-end
- [ ] Port MIDI from shadecore into scheng-input-midi
- [ ] Hot-reload: file watcher → shader recompile without restart
- [ ] State ownership: match shadecore authority table
  (render.json, params.json, output.json, recording.json)

### Phase 5 — Tauri Shell
- [ ] Create crates/scheng-tauri
- [ ] Embed scheng-runtime-wgpu in Tauri Rust backend
- [ ] IPC surface: set_param, load_graph, set_output_mode, start/stop_recording
- [ ] Preview frame IPC: readback → JPEG → Tauri event at ~15fps
- [ ] Instrument template (fork-ready)
- [ ] macOS .app build
- [ ] Windows .exe build
- [ ] Linux build

### Phase 6 — Shader Library (LZX-inspired)
- [ ] Colorizer (hue rotation, RGB↔component)
- [ ] Ramp generator (H, V, radial, angular)
- [ ] Luma keyer / chroma keyer
- [ ] Hard keyer / soft keyer
- [ ] Proc amp (brightness, contrast, saturation, hue)
- [ ] Crossfader with T-bar param
- [ ] Matrix mixer 4→1
- [ ] Video feedback (ping-pong convergence)
- [ ] Pattern generator (color bars, test card, grid)
- [ ] Waveform monitor / vectorscope

### Phase 7 — GUI / Editor (design before building)
- [ ] Research: node graph editor options
- [ ] Define prototyping → export workflow
- [ ] JSON graph save/load format
- [ ] Parameter UI contract (sliders, toggles, XY pads)

---

## CLI Test Reference

```bash
# All tests
cargo test -p scheng-runtime-wgpu -- --nocapture

# CPU-only (no GPU needed)
cargo test -p scheng-runtime-wgpu compat -- --nocapture
cargo test -p scheng-runtime-wgpu test_naga -- --nocapture

# Single GPU test
cargo test -p scheng-runtime-wgpu test_single_node_renders -- --nocapture

# Software fallback (CI / no discrete GPU)
WGPU_BACKEND=gl cargo test -p scheng-runtime-wgpu -- --nocapture

# Contract tests (scheng-graph public API regression)
cargo test -p scheng-contract-tests -- --nocapture

# SDK compat witness (compile-only)
cargo build -p sdk-compat

# Full workspace
cargo build --workspace
cargo test --workspace
```

---

## Architecture Rules (never violate)

1. Engine does not own time — FrameCtx always supplied by host
2. Topology static after compile() — NodeProps/NodeConfig dynamic
3. No layer reaches backward — Graph → Runtime → Backend
4. Control writes NodeProps only — never touches graph or render loop
5. Runtime is single-threaded — GPU context on calling thread only
6. Errors returned immediately — no silent recovery
7. Optional features truly optional — Syphon/Spout/webcam never in core

---

## Open for Phase 1.2

- Custom u_* uniform injection into FrameBlock or a CustomBlock (binding 6)
- Port field names: verify `Port { id, name }` match — executor uses p.id and p.name
- Y-axis flip between OpenGL (bottom-left origin) and wgpu (top-left) — handle in presenter
