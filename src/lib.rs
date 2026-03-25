//! `scheng-runtime-wgpu`
//!
//! A wgpu-based GPU backend for the scheng video synthesis engine.
//! Implements Metal on macOS, DX12 on Windows, and Vulkan on Linux —
//! using the same GLSL shader contract as `scheng-runtime-glow`.
//!
//! # Architecture
//!
//! ```text
//! scheng-graph  (Plan, NodeId, NodeKind)
//!      │
//!      ▼
//! scheng-runtime  (abstract ops, NodeConfig, FrameCtx)
//!      │
//!      ▼
//! scheng-runtime-wgpu  ◄── YOU ARE HERE
//!   ├── WgpuContext     (Device + Queue)
//!   ├── ShaderCache     (GLSL → naga → wgpu ShaderModule)
//!   ├── PipelineCache   (RenderPipeline per node)
//!   ├── RenderTarget    (offscreen wgpu Texture per node)
//!   └── WgpuRuntime     (execute_frame — the main entry point)
//! ```
//!
//! # GLSL Compatibility
//!
//! User shaders are written in GLSL 330 core (matching shadecore convention).
//! The `compat` module preprocesses them before naga compilation:
//!
//! 1. Strips `#version`, `in vec2 v_uv`, `out vec4 fragColor`, standard uniforms
//! 2. Replaces `iChannel0..3` references with split texture+sampler form
//! 3. Prepends the scheng compat header (bindings 0–5)
//!
//! This keeps existing shadecore-style shaders working without modification.
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use scheng_runtime_wgpu::{WgpuRuntime, NodeConfig, FrameCtx};
//! use scheng_graph::{Graph, NodeKind};
//! use std::collections::HashMap;
//!
//! // Build a graph
//! let mut g = Graph::new();
//! let src = g.add_node(NodeKind::ShaderSource);
//! let out = g.add_node(NodeKind::PixelsOut);
//! g.connect_named(src, "out", out, "in").unwrap();
//! let plan = g.compile().unwrap();
//!
//! // Create the runtime (blocking, initialises GPU device)
//! let mut runtime = WgpuRuntime::new(1280, 720).unwrap();
//!
//! // Configure nodes (use builtin shaders for src)
//! let mut configs: HashMap<_, _> = HashMap::new();
//! configs.insert(src, NodeConfig::default());
//! configs.insert(out, NodeConfig::default());
//!
//! // Run one frame
//! let ctx = FrameCtx { width: 1280, height: 720, time: 0.0, frame: 0 };
//! runtime.execute_frame(&plan, &configs, &ctx).unwrap();
//!
//! // Read pixels back for testing
//! let rgba = runtime.readback_pixels(out).unwrap();
//! assert!(!rgba.is_empty());
//! ```

#![warn(missing_docs)]

mod compat;
mod context;
mod executor;
mod pipeline;
mod render_target;
mod shader;
mod uniforms;

// Re-export the public API surface
pub use context::WgpuContext;
pub use executor::{NodeConfig, OutputSink, WgpuRuntime};
pub use render_target::RenderTarget;
pub use shader::ShaderSource;

// Re-export FrameCtx from scheng-core so callers don't need an extra import
// TODO: verify the exact module path once you can check scheng-core's lib.rs
pub use scheng_core::FrameCtx;

use thiserror::Error;

/// Errors produced by the wgpu backend.
#[derive(Debug, Error)]
pub enum WgpuError {
    /// No suitable GPU adapter found.
    #[error("No wgpu adapter found — is a GPU driver installed?")]
    NoAdapter,

    /// Device request failed.
    #[error("wgpu device request failed: {0}")]
    DeviceRequest(#[from] wgpu::RequestDeviceError),

    /// GLSL compilation through naga failed.
    #[error("GLSL compile error in node {node}: {message}")]
    GlslCompile { node: String, message: String },

    /// Naga validation failed.
    #[error("Naga validation error: {0}")]
    NagaValidation(String),

    /// The graph Plan references a node with no config supplied.
    #[error("Missing NodeConfig for node {0:?}")]
    MissingNodeConfig(scheng_graph::NodeId),

    /// A node requires an input that has no upstream render target.
    #[error("Missing upstream render target for input port '{port}' on node {node:?}")]
    MissingInput { node: scheng_graph::NodeId, port: String },

    /// Pixel readback failed because the node has no render target.
    #[error("No render target for node {0:?} — did you run execute_frame first?")]
    NoRenderTarget(scheng_graph::NodeId),

    /// Internal wgpu error.
    #[error("wgpu error: {0}")]
    Wgpu(String),
}
