//! `scheng-param-store` — live parameter state for scheng instruments.
//!
//! `NodeConfig` lives here (not in scheng-runtime-wgpu) to break the
//! dependency cycle. scheng-runtime-wgpu re-exports it for compat.

pub mod schema;
pub mod store;
pub mod builder;
pub mod node_config;

pub use schema::{ParamDef, ParamSchema};
pub use store::ParamStore;
pub use builder::NodeConfigBuilder;
pub use node_config::NodeConfig;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParamError {
    #[error("Failed to read params file '{path}': {source}")]
    Io { path: String, #[source] source: std::io::Error },

    #[error("Failed to parse params JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Unknown parameter name: '{0}'")]
    UnknownParam(String),

    #[error("Unknown MIDI CC: {0}")]
    UnknownMidiCc(u8),
}
