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
// NodeConfig lives in scheng-param-store; re-export it from there.
pub use scheng_param_store::NodeConfig;
pub use scheng_param_store::node_config::PipelineTopology;
pub use executor::{OutputSink, WgpuRuntime};
pub use frame_ctx::FrameCtx;
pub use render_target::RenderTarget;
pub use shader::ShaderSource;

use thiserror::Error;

/// Errors produced by the wgpu backend.
#[derive(Debug, Error)]
pub enum WgpuError {
    #[error("No wgpu adapter found — is a GPU driver installed?")]
    NoAdapter,

    #[error("wgpu device request failed: {0}")]
    DeviceRequest(#[from] wgpu::RequestDeviceError),

    #[error("GLSL compile error in node '{node}': {message}")]
    GlslCompile {
        node:    String,
        message: String,
    },

    #[error("Naga validation error: {0}")]
    NagaValidation(String),

    #[error("Missing NodeConfig for node {0:?}")]
    MissingNodeConfig(scheng_graph::NodeId),

    #[error("Missing upstream render target for port '{port}' on node {node:?}")]
    MissingInput {
        node: scheng_graph::NodeId,
        port: String,
    },

    #[error("No render target for node {0:?} — run execute_frame first")]
    NoRenderTarget(scheng_graph::NodeId),

    #[error("wgpu error: {0}")]
    Wgpu(String),
}
