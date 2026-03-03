<p align="center">
  <img width="35%" height="35%" src="https://raw.githubusercontent.com/schwwaaa/scheng/refs/heads/main/img/logo.png"/>  
</p>

<p align="center"><em>Rust-based engine for GPU-accelerated video synthesis and real-time video instrument development.</em></p> 




---

## What this is


scheng is a modular Rust engine for building real-time video processing pipelines. You describe a directed acyclic graph of nodes — each node is a GLSL shader, a mixer op, or a hardware I/O endpoint — and the engine compiles that graph into an OpenGL render schedule that executes every frame.

```
Real-time GLSL video synthesis and compositing engine. A node-graph signal chain — generators, processors, mixers, and outputs — compiled to OpenGL and rendered live. Built for performance, broadcast-style routing, and LZX-inspired video synthesis.

```

It has no UI of its own. The node graph is authored in a browser-based editor (`scheng-editor.html`) that communicates with the engine over WebSocket. Parameters can be driven from OSC on top of that.

---

## Repository layout

```
crates/
├── scheng-core             Foundation types: config, assets, error, events
├── scheng-graph            Node/port/edge data model. Graph → Plan compilation
├── scheng-runtime          Backend-agnostic ops, parameter blocks, bank/preset system
├── scheng-runtime-glow     OpenGL backend (glow). Shader compiler, FBO manager, frame executor
├── scheng-bridge           WebSocket bridge + visual node graph editor
├── scheng-passes           Ping-pong and temporal ring buffer GPU utilities
├── scheng-buffers          GPU ring buffer primitives
├── scheng-host-winit       Window + GL context creation (winit + glutin)
├── scheng-input-video      Video file decoder → GL texture
├── scheng-input-webcam     Webcam capture → RGBA frames (optional: feature = native)
├── scheng-control-osc      UDP OSC receiver — maps addresses to parameter updates
├── scheng-contract-tests   Golden fixture tests for public SDK contracts
├── scrubbable_controls     JSON-configurable keyboard + OSC control layer
└── sdk-compat              Compile-only witness that the public SDK surface stays usable
```

---

## Crates

### `scheng-core`

Foundation. No GL, no windowing, no runtime.

**`EngineError`** — unified error enum used across all crates. Covers config loading, JSON parse, GL compile/link, and a catch-all `Other`.

**`EngineConfig`** — aggregate of all loaded config files: `render.json`, `params.json`, `output.json`, `recording.json`. Load everything in one call:

```rust
let config = load_engine_config_from(start_dir)?;
```

**`RenderSelection`** — resolved shader path + variant list (for hotkey cycling between shader files). Built from `assets/render.json`.

**`AssetsRoot`** — discovers the `assets/` directory by walking up from a start path.

**`EngineEvent`** — typed event enum (`Log`, `ConfigLoaded`, `ShaderCompileOk`, `ShaderCompileErr`, `Stats`) for structured feedback to UI clients.

**`ConfigMode::Lenient` / `Strict`** — lenient ignores unknown JSON fields and is forward-compatible; strict fails fast on anything unexpected. All public loaders default to lenient.

The config layer is intentionally path-only. The core crate discovers and reads files; callers decide how to validate and deserialize into their own typed structs. This keeps the project modular across CLI, bridge, and future UIs.

---

### `scheng-graph`

The graph data model. No GL, no windowing. `#[forbid(unsafe_code)]`.

Models the LZX mental model: **Sources → Processors / Mixers → Outputs**.

**Node kinds:**

| Class | Kinds |
|---|---|
| Source | `ShaderSource`, `NoiseSource`, `PreviousFrame`, `TextureInputPass`, `VideoDecodeSource` |
| Processor | `ShaderPass`, `ColorCorrect`, `Blur`, `Keyer`, `Feedback` |
| Mixer | `Crossfade`, `Add`, `Multiply`, `KeyMix`, `MatrixMix4` |
| Output | `Window`, `TextureOut`, `PixelsOut`, `Syphon`, `Spout`, `Recorder`, `Ndi`, `Rtsp` |

**Default port conventions by class:**

| Class | Input ports | Output ports |
|---|---|---|
| Source | — | `out` |
| Processor | `in` | `out` |
| Mixer | `a`, `b` | `out` |
| MatrixMix4 | `in0`, `in1`, `in2`, `in3` | `out` |
| Output | `in` | — |

**`Graph`** manages nodes, ports, and edges. Connections are validated: unknown ports, missing nodes, and multiple drivers on one input are all rejected. `Graph::compile()` validates that all Output nodes have their inputs wired and returns a `Plan`.

**`Plan`** — lightweight ordered list of `NodeId`s and edges. Runtimes translate this into backend render schedules.

```rust
let mut g = Graph::new();
let src  = g.add_node(NodeKind::ShaderSource);
let pass = g.add_node(NodeKind::ShaderPass);
let out  = g.add_node(NodeKind::PixelsOut);
g.connect_named(src,  "out", pass, "in")?;
g.connect_named(pass, "out", out,  "in")?;
let plan = g.compile()?;
```

Graph compile is deterministic: compiling the same graph twice produces the same `Plan` node order (pinned by `scheng-contract-tests`).

---

### `scheng-runtime`

Backend-agnostic runtime standard library. No GL, no windowing. `#[forbid(unsafe_code)]`.

**Standard ops:**
- `MixerOp::Crossfade` — 2-input blend
- `MixerOp::Add` — additive blend
- `MixerOp::Multiply` — multiply blend
- `MixerOp::MatrixMix4` — weighted sum of up to 4 inputs via `iChannel0..3`

**Parameter blocks:**
- `MixerParams { mix: f32 }` — crossfade position. 0.0 = full A, 1.0 = full B
- `MatrixMixParams { weights: [f32; 4] }` — per-channel gains. Default `[1, 0, 0, 0]` passes channel 0

**`MatrixPreset`** — named routing presets: `Solo0/1/2/3`, `Quad` (equal blend), `Sum01`, `Sum23`. Deterministic, backend-agnostic. Suitable for scene/bank systems.

**Bank and scene system:**
- `SceneDef { name, preset }` — a named matrix routing scene
- `BankDef { name, scenes }` — a named collection of scenes
- `BankSet` — a validated set of banks. Load from JSON: `BankSet::from_json_path(path)`. `BankSet::builtin_matrix_banks()` provides a standard set.

**`runtime_contract` module:**

`input_channel_for(kind, port_name) -> Option<u32>` — canonical port → texture unit mapping:

| Port name | Texture unit |
|---|---|
| `"in"`, `"in0"`, `"a"`, `"src"` | `iChannel0` |
| `"in1"`, `"b"`, `"src1"` | `iChannel1` |
| `"in2"`, `"c"`, `"src2"` | `iChannel2` |
| `"in3"`, `"d"`, `"src3"` | `iChannel3` |

`plan_output_names(pixels_out)` — validates multi-output graphs: exactly one unnamed `PixelsOut` (the primary), all others must be uniquely named. `"main"` is reserved.

`builtin_shader_for(kind)` — default vertex + fragment GLSL for each node kind. The vertex shader declares `out vec2 v_uv` (lowercase). All custom fragment shaders must match with `in vec2 v_uv`.

---

### `scheng-runtime-glow`

The OpenGL backend, built on [`glow`](https://github.com/grovesNL/glow).

**Responsibilities — and only these:**
- Compile and link GLSL programs from `ShaderSource { vert, frag }`
- Manage offscreen `RenderTarget`s (FBO + color texture)
- Bind input textures to `iChannel0`…`iChannel3` before each pass
- Execute a compiled `Plan` frame by frame via `execute_plan_to_sink()`
- Manage ping-pong buffers for `PreviousFrame` / `Feedback` nodes
- Decode and upload video frames from `VideoDecodeSource` nodes

Does **not** contain: windowing, file I/O policy, hot-reload, MIDI/OSC, recording, or sinks. These belong to host crates.

**`FrameCtx { width, height, time, frame }`** — the engine does not own time. The host supplies a `FrameCtx` each frame. `time` is seconds since start (bound to `uTime`); `frame` is a monotonic counter.

**`OutputSink`** — trait implemented by the host to consume the rendered output. The main binary blits to the window framebuffer. Other implementations could write to an NDI stream, a video encoder, or a Syphon server.

**GLSL contract (fragment shaders):**

```glsl
#version 330 core
in vec2 v_uv;             // ← must be v_uv (lowercase) — matches vertex out
out vec4 fragColor;       // output name is flexible

uniform sampler2D iChannel0;  // input textures up to iChannel3
uniform float uTime;          // seconds since start (also u_time)
uniform vec2 uResolution;     // output dimensions (also u_resolution)
uniform float u_myParam;      // custom uniforms — use u_ prefix by convention
```

**Syphon output (macOS):** Build with `--features syphon`. Requires `vendor/Syphon.framework` at workspace root. `build.rs` compiles the Objective-C bridge in `native/syphon_bridge.m` via `cc` and links the framework with correct `rpath` entries for both debug and release.

---

### `scheng-bridge`

WebSocket server that exposes the engine to the browser editor and any external tool. Uses tokio + tokio-tungstenite.

Listens on `ws://127.0.0.1:7777` by default. **All connected clients receive all broadcast events simultaneously** — multiple browser windows, OSC adapters, or recording tools can observe the same engine state.

**Run standalone:**

```bash
cargo run -p scheng-bridge
SCHENG_BRIDGE_ADDR=0.0.0.0:7777 cargo run -p scheng-bridge
RUST_LOG=scheng_bridge=debug cargo run -p scheng-bridge
```

**Embed in an existing binary:**

```rust
use scheng_bridge::BridgeServer;

let bridge = BridgeServer::new("127.0.0.1:7777".parse()?);
let manager = bridge.manager.clone(); // share with render loop
tokio::spawn(async move { bridge.run().await.unwrap(); });

// In render loop:
manager.advance_frame(1920, 1080, elapsed_secs, frame_num)?;
```

**Wiring to the real engine** — `graph_manager.rs` contains clearly marked stubs. Replace the three stub blocks (`compile`, `advance_frame`, and `NodeProps` sync) with real engine calls. Full instructions in `INTEGRATION.md` inside the crate.

**Protocol — inbound (`action` field routes commands):**

| `action` | key fields | description |
|---|---|---|
| `add_node` | `id`, `kind`, `label`, `position?` | Add a node |
| `remove_node` | `id` | Remove node + all edges |
| `connect` | `from_node`, `from_port`, `to_node`, `to_port` | Wire two ports |
| `disconnect` | same | Remove an edge |
| `set_shader` | `node_id`, `glsl` | Replace GLSL source |
| `set_param` | `node_id`, `param`, `value` | Update one parameter |
| `set_params` | `node_id`, `params` | Batch parameter update |
| `compile` | — | Validate topology + build execution plan |
| `get_state` | — | Request full state snapshot |
| `advance_frame` | `width`, `height`, `time`, `frame` | Drive engine one frame forward |
| `move_node` | `id`, `position` | Update canvas position (UI only) |

**Protocol — outbound (`event` field identifies messages):**

| `event` | description |
|---|---|
| `state` | Full graph + params snapshot. Sent on connect and after compile |
| `node_added` / `node_removed` | Echoed topology changes |
| `edge_added` / `edge_removed` | Echoed edge changes |
| `compiled` | Compile succeeded — `node_count`, `edge_count` |
| `shader_updated` | GLSL source replaced |
| `param_updated` | Single parameter changed |
| `frame_executed` | Frame complete — `elapsed_ms` |
| `error` | Command rejected or engine error — `message`, `context?` |
| `ok` | Generic acknowledgement — `ack` |

**`ParamValue`** is an untagged JSON union: `Float(f32)`, `Int(i64)`, `Bool(bool)`, `Vec2([f32;2])`, `Vec3`, `Vec4`, `Text(String)`.

**Security:** binds loopback by default. Set `SCHENG_BRIDGE_ADDR=0.0.0.0:7777` for network access. Add auth/TLS before exposing outside localhost.

---

### `scheng-editor.html`

Single-file browser-based node graph editor. Zero dependencies, no build step. Open directly in Chrome or Firefox.

Communicates with `scheng-bridge` over WebSocket. Lets you spawn nodes, draw wires, load shaders from a 128-shader FX library, adjust uniforms with live sliders, and save/load patches.

**Patch files** are downloaded as `.scheng.json` — JSON containing `nodes[]`, `edges[]`, uniforms, and raw GLSL. Nothing is stored in the browser.

For complete editor documentation see `scheng-editor-README.md`.

---

### `scheng-passes`

GPU utility helpers built on top of `scheng-runtime-glow`.

**`PingPongTarget`** — two `RenderTarget`s that swap roles each frame. `prev_tex()` gives the previous frame's texture for feedback sampling; `next_target()` gives the FBO to render into this frame. Initialized to black to prevent undefined sampling on frame 0.

**`TemporalRing`** — a ring buffer of N `RenderTarget`s. `push_from_fbo()` blits into the next slot via `glBlitFramebuffer`. Useful for multi-tap video delay, motion blur accumulation, or any effect that needs N frames of history.

---

### `scheng-buffers`

Lower-level GPU buffer primitives. Currently re-exports `TemporalRing` for use by examples and higher-level crates.

---

### `scheng-host-winit`

Window and GL context creation via winit + glutin. Currently a stub `Host` struct — will grow into a full context-creation helper as the windowing layer matures. Kept as a separate crate so `scheng-runtime-glow` stays embed-friendly with no windowing dependency.

---

### `scheng-input-video`

Video file decoder that uploads frames to OpenGL textures. Maps `FrameCtx::time` (seconds) to a frame index using the clip's nominal fps. Used internally by `scheng-runtime-glow` for `VideoDecodeSource` nodes.

---

### `scheng-input-webcam`

Webcam capture via [nokhwa](https://github.com/l1npengtul/nokhwa). Gated behind `features = ["native"]` — builds cleanly on all platforms without it, returns `WebcamError::NotEnabled` at runtime.

```rust
// Build with: --features native
let mut cam = Webcam::new(0, 1280, 720)?;
let frame: RgbaFrame = cam.poll_rgba()?;
// frame.bytes: RGBA8, frame.width, frame.height
```

---

### `scheng-control-osc`

Minimal non-blocking UDP OSC receiver. Drain the socket each frame with `poll()`.

Address convention — either form resolves to the parameter name:
- `/param/<name>`
- `/<name>`

First argument is coerced to `f32` from Float, Double, Int, or Long.

```rust
let mut osc = OscParamReceiver::bind("127.0.0.1:9000")?;

// In render loop:
for (name, value) in osc.poll() {
    engine.set_param(&name, value);
}
```

---

### `scrubbable_controls`

JSON-configurable keyboard and OSC control layer for examples and instruments. Keeps the control plane completely separate from the engine graph.

**`ControlLayer`** owns:
- `TransportState` — `speed`, `paused`, `norm_pos`, `scrub_delta`
- `ColorState` — `brightness`, `contrast`, `saturation`
- A `Keymap` loaded from `keymap.json`
- An `OscMap` loaded from `osc_map.json`

```rust
let mut controls = ControlLayer::load("keymap.json", "osc_map.json")?;

// On keyboard event (e.g. from winit):
controls.on_key("space");

// On OSC event:
controls.on_osc(OscMessage {
    addr: "/scheng/transport/speed".into(),
    args: vec![0.5],
});

// In render loop — feed directly to shader uniforms:
shader.set_uniform("u_speed",      controls.transport.speed);
shader.set_uniform("u_norm_pos",   controls.transport.norm_pos);
shader.set_uniform("u_brightness", controls.color.brightness);
shader.set_uniform("u_contrast",   controls.color.contrast);
```

Key and OSC mappings are reconfigurable in JSON without recompiling. See `keymap.json` and `osc_map.json` for reference layouts.

---

### `scheng-contract-tests`

Integration tests that pin the public SDK surface against golden JSON fixtures and behavioral contracts.

**Golden fixture tests:**
- `banks_builtin.json` — deserializes correctly, has at least one bank with at least one scene
- `banks_empty.json` — rejected with a message mentioning "banks" / "empty"
- `banks_missing_key.json` — rejected with a message mentioning "missing" / "key"
- `banks_bad_preset.json` — rejected with a message mentioning "unknown preset"

**Output naming contracts:**
- Zero `PixelsOut` nodes → rejected
- Two unnamed `PixelsOut` → rejected (ambiguous primary)
- Explicit name `"main"` → rejected (reserved)
- Duplicate explicit names → rejected
- One unnamed + N named → accepted, primary and named outputs correctly identified

**Determinism:** compiling the same `Graph` twice produces the same `Plan` node order.

Run with `cargo test -p scheng-contract-tests`.

---

### `sdk-compat`

A compile-only witness crate. Contains a `_compile_witness()` function that exercises `scheng-graph`, `scheng-runtime`, and their public APIs — without running anything. CI builds this crate to catch accidental breaking changes. If it stops compiling, the public SDK surface has a regression.

---

## Building

**Requirements:**
- Rust stable (2021 edition)
- OpenGL 3.3+ capable GPU and driver
- macOS, Linux, or Windows

```bash
# Build everything
cargo build --workspace

# Run the bridge (editor + engine window)
cargo run -p scheng-bridge

# With logging
RUST_LOG=scheng_bridge=debug cargo run -p scheng-bridge

# Custom WebSocket address
SCHENG_BRIDGE_ADDR=0.0.0.0:7777 cargo run -p scheng-bridge

# Run contract tests
cargo test -p scheng-contract-tests

# Run all tests
cargo test --workspace
```

**macOS + Syphon output:**

```bash
# Place Syphon.framework in vendor/Syphon.framework at workspace root, then:
cargo build -p scheng-runtime-glow --features syphon
```

`build.rs` in `scheng-runtime-glow` compiles the Objective-C bridge (`native/syphon_bridge.m`) via the `cc` crate and links the framework with correct `rpath` entries for both debug and release builds.

**Webcam input:**

```bash
cargo build -p scheng-input-webcam --features native
```

---

## Quick start

1. `cargo run -p scheng-bridge`
2. Open `crates/scheng-bridge/scheng-editor.html` in Chrome or Firefox
3. Click **connect** — status dot turns green
4. Click any template from the templates panel (e.g. **T-Bar Crossfader**)
5. Click **▶ compile** — video renders in the bridge window

---

## OSC control

Every uniform slider in the editor is addressable over OSC:

```
/scheng/node/<node_id>/uniform/<uniform_name>  <float>
```

Short form:

```
/<node_label>/<uniform_name>  <float>
```

Examples:

```
/scheng/node/xfad/uniform/u_tbar  0.75
/scheng/node/key/uniform/u_thresh  0.35
```

The full OSC address is shown as a tooltip on every slider in the editor.

`scheng-control-osc` handles the UDP receive side. Wire its `poll()` output to `set_param` calls in your render loop.

---

## Design principles

**The engine does not own time.** `FrameCtx` is supplied by the host each frame. This makes the engine trivially embeddable and testable — the bridge accepts `advance_frame` messages; a unit test can advance frame-by-frame with controlled timestamps.

**The bridge never reaches into engine internals.** It calls public SDK surface only. Stubs in `graph_manager.rs` are clearly marked and fully documented in `INTEGRATION.md`.

**Node kinds are stable contracts.** `scheng-graph::NodeKind`, default port names, and `runtime_contract::input_channel_for` are pinned by `scheng-contract-tests`. Changing them is a breaking change.

**Configuration is forward-compatible by default.** `ConfigMode::Lenient` ignores unknown JSON fields. Opt into `Strict` for fail-fast validation during development.

**Optional features are truly optional.** Syphon and webcam build cleanly without their feature flags — neither bleeds into the base crate graph.

**No unnecessary coupling.** Each crate has one job. `scheng-runtime-glow` does not know about windows. `scheng-host-winit` does not know about shaders. `scheng-control-osc` does not know about the graph.

---

## Dependency graph

```
scheng-editor.html
        │  WebSocket JSON
        ▼
scheng-bridge  (tokio WebSocket, broadcast fan-out)
  graph_manager.rs  (instrument layer — stubs → real engine calls)
        │
        ├── scheng-runtime-glow  (OpenGL backend)
        │     ├── scheng-runtime  (ops, params, banks, contract)
        │     │     └── scheng-graph  (node/port/edge/plan)
        │     │           └── scheng-core  (error, config, events)
        │     └── scheng-input-video  (file decoder → GL texture)
        │
        └── glow  (OpenGL bindings)

scheng-passes       (PingPongTarget, TemporalRing — on top of runtime-glow)
scheng-buffers      (GPU ring buffer primitives)
scheng-host-winit   (window + GL context — winit + glutin)
scheng-input-webcam (camera capture — optional: native feature)
scheng-control-osc  (UDP OSC receiver — rosc)
scrubbable_controls (keyboard + OSC control layer — JSON configurable)
scheng-contract-tests (golden fixture + behavioral contract tests)
sdk-compat          (compile-only API witness)
```
