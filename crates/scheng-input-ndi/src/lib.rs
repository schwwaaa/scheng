//! `scheng-input-ndi`
//!
//! NDI receive → wgpu RGBA texture for scheng instruments.
//!
//! NDI sources on the local network are discoverable and can be used as
//! live video inputs to any node in the graph.
//!
//! # Status
//!
//! Interface defined, SDK stub ready. Wire in your preferred NDI Rust
//! crate (ndi-rs or similar) and fill in the TODOs in `receiver.rs`.
//!
//! # Usage
//!
//! ```rust,ignore
//! use scheng_input_ndi::NdiReceiver;
//!
//! // Find sources on the network (blocks briefly for discovery)
//! let sources = NdiReceiver::find_sources(2000)?;
//! println!("Found: {:?}", sources);
//!
//! // Open the first source
//! let mut recv = NdiReceiver::open(&sources[0], &device, &queue)?;
//!
//! // In render loop
//! recv.poll(&queue);
//! let view = recv.texture_view();
//! ```

pub mod receiver;
pub use receiver::NdiReceiver;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NdiError {
    #[error("NDI SDK not available — install the NDI SDK and add SDK bindings")]
    SdkNotFound,

    #[error("NDI source '{name}' not found or timed out")]
    SourceNotFound { name: String },

    #[error("NDI receive error: {0}")]
    ReceiveError(String),
}
