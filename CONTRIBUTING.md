# Contributing to scheng

Thank you for your interest in contributing. scheng is a Rust SDK for GPU-accelerated video synthesis and contributions of all kinds are welcome — bug fixes, new I/O crates, shader examples, documentation, and tests.

---

## Table of contents

- [Workspace setup](#workspace-setup)
- [Running tests](#running-tests)
- [Crate ownership](#crate-ownership)
- [Pull request guidelines](#pull-request-guidelines)
- [Code style](#code-style)
- [Adding a new I/O crate](#adding-a-new-io-crate)
- [Architecture rules](#architecture-rules)

---

## Workspace setup

**Requirements**

- Rust 1.75+
- ffmpeg 6+ in PATH
- macOS: `vendor/Syphon.framework` for Syphon crates
- NDI SDK at `/Library/NDI SDK for Apple` (macOS) for NDI crates

**Clone and build**

```bash
git clone https://github.com/yourusername/scheng
cd scheng
cargo build --workspace --exclude scheng-example-instrument
```

**macOS — Syphon setup**

Download Syphon framework and place it at:
```
scheng/vendor/Syphon.framework
```

---

## Running tests

```bash
# All workspace tests (headless GPU — no display required)
cargo test --workspace --exclude scheng-example-instrument

# Single crate
cargo test -p scheng-runtime-wgpu

# With output
cargo test -p scheng-runtime-wgpu -- --nocapture

# Check warnings
cargo clippy --workspace --exclude scheng-example-instrument
```

Tests use wgpu's headless Metal/Vulkan backend. No window or display server required. For CI without a real GPU, set `WGPU_BACKEND=gl`.

**Zero warnings policy** — all crates must compile warning-free. Run `cargo fix` before opening a PR.

---

## Crate ownership

Understanding crate boundaries prevents the most common architectural mistakes.

| Crate | Owns | Does NOT own |
|-------|------|--------------|
| `scheng-graph` | Node topology, port definitions, `compile()` | Shader compilation, GPU resources, execution |
| `scheng-runtime-wgpu` | `execute_frame()`, render targets, pipeline cache, `CustomBlock` | Topology mutation, control protocols, time |
| `scheng-param-store` | ParamStore, schema, MIDI/OSC routing, smoothing | Protocol parsing, device management |
| `scheng-hotreload` | File watcher, change events | Shader compilation |
| Input crates | Device polling, texture upload | Render pipeline, param routing |
| Output crates | Frame delivery (sink trait) | Graph mutation, render scheduling |

**The key rule:** No crate reaches backward across a boundary. Input crates don't touch the runtime internals. The runtime doesn't parse MIDI. Control never executes shaders directly.

When ownership is unclear: does it belong in the graph (topology), NodeConfig (per-frame config), runtime (execution), or the instrument layer? If "instrument layer" — it probably doesn't belong in the SDK at all.

---

## Pull request guidelines

1. **Open an issue first** for anything non-trivial. Alignment before code saves everyone time.
2. **One concern per PR.** Bug fix, feature, or refactor — not all three.
3. **Tests required** for changes to core crates (`scheng-graph`, `scheng-runtime-wgpu`, `scheng-param-store`).
4. **Zero warnings.** `cargo clippy` must pass clean.
5. **Update the changelog.** Add your change to `CHANGELOG.md` under `[Unreleased]`.
6. **Describe the "why".** PR description should explain motivation, not just what changed.

### PR title format

```
feat: add SpoutSink for Windows DX texture sharing
fix: correct Y-flip in webcam passthrough shader
docs: add NDI receive code example to developer reference
refactor: extract CustomBlock buffer creation into uniforms.rs
test: add headless test for Crossfade node with u_tbar=0.5
```

---

## Code style

- **Rust edition 2021**
- **`rustfmt`** defaults — run `cargo fmt` before committing
- **Log levels:** `log::info!` for startup events, `log::warn!` for degraded operation, `log::error!` for failures. Prefix with a tag in brackets: `[MIDI]`, `[SYPHON]`, `[OUTPUT]`
- **Error handling:** return `Result<_, E>` from fallible functions. Avoid `unwrap()` outside tests and examples
- **Naming:** follow Rust conventions. Acronyms in type names: `WgpuRuntime`, `SyphonSink`, `NdiReceiver`
- **Comments:** explain *why*, not *what*. The code shows what; the comment shows intent

---

## Adding a new I/O crate

To add a new input or output protocol:

1. Create `crates/scheng-input-myprotocol/` or `crates/scheng-output-myprotocol/`
2. Implement the appropriate trait:
   - Inputs: expose `texture_arc() -> Option<Arc<wgpu::Texture>>` and a `poll()` method
   - Outputs: implement `OutputSink` from `scheng-runtime-wgpu`
3. Add to workspace `Cargo.toml` members
4. Add a feature flag if the crate has a native dependency (see `scheng-input-ndi` as a model)
5. Add at least one integration test
6. Document in `CHANGELOG.md`

Platform-specific crates must gate with `#[cfg(target_os)]` and must not add compile errors on other platforms.

---

## Architecture rules

These are not preferences — they are constraints that protect the SDK's core guarantees.

**Topology is static, configuration is dynamic.**
The graph structure is fixed after `compile()`. `NodeConfig` changes every frame. Never conflate them.

**The engine does not own time.**
No play/pause, no frame clock, no transport logic in core crates. All temporal semantics come from `FrameCtx` supplied by the instrument.

**The engine is protocol-agnostic.**
No OSC parsing, no MIDI handling, no network sockets in `scheng-graph` or `scheng-runtime-wgpu`. Control values enter only through `NodeConfig.uniforms`.

**Node kinds must remain minimal.**
Before adding a new `NodeKind`, ask: can this be achieved with a new shader or `NodeConfig` parameter? If yes, don't add the node kind.

**No per-frame allocation in hot paths.**
`execute_frame()` must not allocate on the hot path. GPU buffer creation, shader compilation, and render target allocation are acceptable on the first frame for a given configuration, then must be cached.

---

## Need help?

Open an issue or start a discussion on GitHub. We're happy to talk through architecture questions before you write code.
