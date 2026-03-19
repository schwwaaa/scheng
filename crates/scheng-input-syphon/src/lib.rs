//! `scheng-input-syphon`
//!
//! Syphon Metal receive → wgpu RGBA texture. macOS only.
//!
//! Connects to a named Syphon server (OBS, VDMX, Resolume, another scheng
//! instrument) and pulls the latest frame each render cycle.
//!
//! # Setup
//!
//! Place `Syphon.framework` at `<workspace>/vendor/Syphon.framework`.
//! Download: https://github.com/Syphon/Syphon-Framework/releases
//!
//! # Usage
//!
//! ```rust,ignore
//! use scheng_input_syphon::SyphonReceiver;
//!
//! // List available servers
//! let servers = SyphonReceiver::list_servers();
//! println!("Available: {:?}", servers);
//!
//! // Connect to a server by name
//! let mut recv = SyphonReceiver::connect("OBS", mtl_device_ptr, &device, &queue)?;
//!
//! // In render loop — poll for new frame, upload to wgpu texture
//! recv.poll(&queue);
//! let view = recv.texture_view(); // bind as iChannel0
//! ```

pub mod ffi;
pub mod receiver;

pub use receiver::{SyphonReceiver, SyphonServerInfo};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyphonInputError {
    #[error("Syphon input is only available on macOS")]
    NotMacOs,

    #[error("Syphon server '{name}' not found — available: {available:?}")]
    ServerNotFound { name: String, available: Vec<String> },

    #[error("Failed to connect to Syphon server '{name}'")]
    ConnectFailed { name: String },

    #[error("Frame pull failed")]
    PullFailed,
}
