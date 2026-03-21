//! `scheng-input-ndi` — NDI source receiver → wgpu texture.
//!
//! Discover and receive NDI sources on the local network as live video
//! inputs to any node in the scheng graph.
//!
//! # Requirements
//! Install NDI 6 SDK: https://ndi.video/download-ndi-sdk/
//!
//! # Enabling
//! ```toml
//! scheng-input-ndi = { path = "...", features = ["ndi"] }
//! ```
//!
//! # Usage
//! ```rust,ignore
//! use scheng_input_ndi::NdiReceiver;
//!
//! let sources = NdiReceiver::find_sources(2000)?;
//! println!("Found: {:?}", sources);
//!
//! let mut recv = NdiReceiver::open(&sources[0], &device, &queue)?;
//!
//! // In render loop:
//! recv.poll(&device, &queue);
//! if let Some(view) = recv.texture_view() {
//!     // bind as iChannel0
//! }
//! ```

pub mod receiver;
pub use receiver::{NdiReceiver, NdiSource};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NdiError {
    #[error("NDI SDK not available — install from ndi.video and enable the 'ndi' feature")]
    SdkNotFound,

    #[error("NDI source '{name}' not found or timed out")]
    SourceNotFound { name: String },

    #[error("NDI receive error: {0}")]
    ReceiveError(String),
}
