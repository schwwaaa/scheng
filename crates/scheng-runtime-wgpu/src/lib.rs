//! `scheng-runtime-wgpu` — wgpu backend for the scheng video engine.
#![allow(dead_code)]

pub mod compat;
pub mod context;
pub mod executor;
pub mod frame_ctx;
pub mod pipeline;
pub mod render_target;
pub mod shader;
pub mod uniforms;
pub mod plugin;

pub use context::WgpuContext;
pub use executor::{NodeConfig, OutputSink, WgpuRuntime};
pub use frame_ctx::FrameCtx;
pub use render_target::RenderTarget;
pub use shader::ShaderSource;

use thiserror::Error;

/// Errors produced by the wgpu backend.
#[derive(Debug, Error)]
pub enum WgpuError {
    /// No suitable GPU adapter found.
    #[error("No wgpu adapter found — is a GPU driver installed?")]
    NoAdapter,

    /// wgpu device request failed.
    #[error("wgpu device request failed: {0}")]
    DeviceRequest(#[from] wgpu::RequestDeviceError),

    /// GLSL compilation through naga failed.
    #[error("GLSL compile error in node '{node}': {message}")]
    GlslCompile {
        /// Label of the node that failed.
        node: String,
        /// Error messages from naga.
        message: String,
    },

    /// Naga IR validation failed.
    #[error("Naga validation error: {0}")]
    NagaValidation(String),

    /// A node in the Plan has no entry in the configs map.
    #[error("Missing NodeConfig for node {0:?}")]
    MissingNodeConfig(scheng_graph::NodeId),

    /// An input port has no upstream render target.
    #[error("Missing upstream render target for port '{port}' on node {node:?}")]
    MissingInput {
        /// The node missing its input.
        node: scheng_graph::NodeId,
        /// The port name that had no upstream connection.
        port: String,
    },

    /// Pixel readback requested before execute_frame has run.
    #[error("No render target for node {0:?} — run execute_frame first")]
    NoRenderTarget(scheng_graph::NodeId),

    /// Internal wgpu error.
    #[error("wgpu error: {0}")]
    Wgpu(String),
}
