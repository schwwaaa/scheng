//! `plugin.rs` — scheng plugin contracts.
//!
//! This module defines the traits that third-party crates must implement
//! to integrate with the scheng SDK as first-class inputs or outputs.
//!
//! # Overview
//!
//! scheng has two plugin surfaces:
//!
//! - **[`InputSource`]** — produces GPU textures from an external source
//!   (camera, network stream, hardware capture, generative source, etc.)
//! - **[`OutputSink`]** — consumes rendered frames and delivers them
//!   somewhere (display protocol, encoder, network, file, etc.)
//!
//! Both traits are intentionally minimal. The SDK provides all GPU
//! resource management — plugin authors only implement the protocol-specific
//! logic at the edges.
//!
//! # Implementing an InputSource
//!
//! ```rust
//! use std::sync::Arc;
//! use scheng_runtime_wgpu::plugin::InputSource;
//!
//! pub struct MyReceiver {
//!     texture: Arc<wgpu::Texture>,
//!     width:   u32,
//!     height:  u32,
//! }
//!
//! impl InputSource for MyReceiver {
//!     fn poll(&mut self, _device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
//!         // Fetch latest frame, upload to self.texture, return true if updated
//!         true
//!     }
//!     fn texture_arc(&self) -> Option<Arc<wgpu::Texture>> {
//!         Some(Arc::clone(&self.texture))
//!     }
//!     fn width(&self)  -> u32 { self.width  }
//!     fn height(&self) -> u32 { self.height }
//!     fn name(&self)   -> &str { "my-receiver" }
//! }
//! ```
//!
//! # Implementing an OutputSink
//!
//! ```rust
//! use scheng_runtime_wgpu::plugin::OutputSink;
//! use scheng_runtime_wgpu::{RenderTarget, FrameCtx};
//! use scheng_graph::NodeId;
//!
//! pub struct MyOutput { /* ... */ }
//!
//! impl OutputSink for MyOutput {
//!     fn present(
//!         &mut self,
//!         _node_id: NodeId,
//!         target:   &RenderTarget,
//!         ctx:      &FrameCtx,
//!         device:   &wgpu::Device,
//!         queue:    &wgpu::Queue,
//!     ) {
//!         // Read pixels, share texture, push to encoder, etc.
//!         let _ = (target, ctx, device, queue);
//!     }
//! }
//! ```
//!
//! # Naming convention
//!
//! | Crate type | Naming pattern | Example |
//! |------------|---------------|---------|
//! | Input source | `scheng-input-{protocol}` | `scheng-input-blackmagic` |
//! | Output sink  | `scheng-output-{protocol}` | `scheng-output-spout` |
//! | Utility      | `scheng-{name}` | `scheng-sdf` |
//!
//! # Versioning
//!
//! Declare your supported scheng version range in `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! scheng-runtime-wgpu = "0.1"   # semver — patch updates are always compatible
//! ```
//!
//! The `InputSource` and `OutputSink` traits follow semantic versioning.
//! Breaking changes to these traits will increment the major version.

use std::sync::Arc;

use crate::{FrameCtx, RenderTarget};
use scheng_graph::NodeId;

// ── InputSource ──────────────────────────────────────────────────────────────

/// Trait for all scheng-compatible input sources.
///
/// An `InputSource` produces a `wgpu::Texture` that can be injected into
/// the render graph as `iChannel0–3` on any `ShaderSource` or `ShaderPass`
/// node via [`NodeConfig::input_textures`].
///
/// # Threading
///
/// `InputSource` is **not** required to be `Send + Sync`. Implementations
/// that run capture loops on background threads (webcam, NDI, RTMP) should
/// do so internally and expose only the texture side here.
///
/// The `poll()` method is always called from the **GPU render thread**,
/// immediately before `execute_frame()`.
///
/// # Allocation
///
/// Allocate the `wgpu::Texture` once in the constructor and reuse it
/// across frames. `poll()` should only upload new pixel data to the
/// existing texture — not reallocate it.
///
/// Resolution changes (e.g. dynamic stream size changes) should reallocate
/// once and log a warning, then resume with the new size.
pub trait InputSource {
    /// Upload the latest available frame to the GPU texture.
    ///
    /// Called once per tick from the render thread, before `execute_frame()`.
    ///
    /// Returns `true` if a new frame was available and uploaded,
    /// `false` if no new data was ready (previous frame remains valid).
    ///
    /// Must not block. If no frame is available, return `false` immediately.
    fn poll(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool;

    /// Return the current GPU texture for injection into the render graph.
    ///
    /// Returns `None` only before the first frame has been received.
    /// After the first successful `poll()`, this should always return `Some`.
    ///
    /// The returned `Arc<wgpu::Texture>` is assigned to
    /// `NodeConfig::input_textures[N]` and sampled in GLSL as `iChannelN`.
    fn texture_arc(&self) -> Option<Arc<wgpu::Texture>>;

    /// Width of the current texture in pixels.
    fn width(&self) -> u32;

    /// Height of the current texture in pixels.
    fn height(&self) -> u32;

    /// Human-readable name for logging and diagnostics.
    ///
    /// Should be short and stable (e.g. `"ndi-receive"`, `"syphon-in"`).
    fn name(&self) -> &str;

    /// Whether this source is currently connected and delivering frames.
    ///
    /// Default implementation returns `true` if `texture_arc()` is `Some`.
    /// Override if your source has a meaningful connected/disconnected state.
    fn is_connected(&self) -> bool {
        self.texture_arc().is_some()
    }

    /// Texture format delivered by this source.
    ///
    /// Must match the format of the texture returned by `texture_arc()`.
    /// Defaults to `Rgba8Unorm` — the format used by all built-in inputs.
    /// Override only if your source delivers a different format.
    fn texture_format(&self) -> wgpu::TextureFormat {
        wgpu::TextureFormat::Rgba8Unorm
    }
}

// ── OutputSink ───────────────────────────────────────────────────────────────

/// Trait for all scheng-compatible output sinks.
///
/// An `OutputSink` receives the final rendered frame from a `PixelsOut` node
/// and delivers it somewhere — a window, a network protocol, an encoder,
/// a hardware output, etc.
///
/// # Execution order
///
/// `present()` is called **after** `queue.submit()` on every frame.
/// The render target's texture has been fully written by the GPU before
/// `present()` is invoked. Never call `present()` before `queue.submit()`.
///
/// # Multiple sinks
///
/// Multiple `OutputSink` implementations can be active simultaneously.
/// Call `execute_frame()` once per sink per frame. The graph is re-evaluated
/// for each call, but the pipeline cache ensures zero recompilation cost.
///
/// # GPU commands
///
/// Do **not** issue new `wgpu` render commands from inside `present()`.
/// The command encoder has already been submitted. Use `queue.write_buffer()`
/// or `queue.write_texture()` for lightweight GPU work only.
pub trait OutputSink {
    /// Deliver the rendered frame to the output destination.
    ///
    /// # Parameters
    ///
    /// - `node_id` — The `PixelsOut` node that triggered this sink.
    ///   Use this to distinguish between multiple output nodes in one graph.
    /// - `target` — The render target holding the completed frame.
    ///   Access `target.sample_view` for the resolved (non-MSAA) texture view.
    ///   Access `target.width` / `target.height` for frame dimensions.
    /// - `ctx` — The `FrameCtx` for this frame (time, frame index, resolution).
    /// - `device` / `queue` — For lightweight GPU operations only.
    ///   Do not record new render passes.
    fn present(
        &mut self,
        node_id: NodeId,
        target:  &RenderTarget,
        ctx:     &FrameCtx,
        device:  &wgpu::Device,
        queue:   &wgpu::Queue,
    );

    /// Human-readable name for logging and diagnostics.
    ///
    /// Should be short and stable (e.g. `"syphon-out"`, `"ndi-out"`).
    fn name(&self) -> &str {
        "output-sink"
    }

    /// Called when the instrument is shutting down.
    ///
    /// Override to flush buffers, close network connections, finalize
    /// recordings, etc. Default implementation does nothing.
    fn shutdown(&mut self) {}
}

// ── PluginInfo ────────────────────────────────────────────────────────────────

/// Metadata describing a scheng plugin crate.
///
/// Not required — plugins do not need to implement this. It is provided
/// as a convention for plugin authors who want to expose version and
/// capability information at runtime.
#[derive(Debug, Clone)]
pub struct PluginInfo {
    /// Crate name (e.g. `"scheng-input-blackmagic"`).
    pub name:            &'static str,
    /// Semver version string (e.g. `"0.1.0"`).
    pub version:         &'static str,
    /// Human description of what this plugin does.
    pub description:     &'static str,
    /// Minimum scheng SDK version this plugin requires.
    pub min_sdk_version: &'static str,
    /// Target platforms (e.g. `&["macos", "windows"]`).
    pub platforms:       &'static [&'static str],
}
