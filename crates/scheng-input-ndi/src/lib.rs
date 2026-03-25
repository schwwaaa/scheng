//! `scheng-input-ndi` — NDI source receiver → wgpu RGBA texture.
//!
//! Discover and receive NDI sources on the local network as live video
//! inputs to any node in the scheng graph.
//!
//! # Requirements
//! Install NDI 6 SDK from https://ndi.video/download-ndi-sdk/
//! macOS: /Library/NDI SDK for Apple  (or set NDI_SDK_DIR)
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
//! // Discover sources (blocks for timeout_ms)
//! let sources = NdiReceiver::find_sources(2000)?;
//! for s in &sources { println!("  {}", s.name); }
//!
//! // Open a source
//! let mut recv = NdiReceiver::open(&sources[0], &device, &queue)?;
//!
//! // In render loop:
//! if recv.poll(&device, &queue) {
//!     if let Some(view) = recv.texture_view() {
//!         // bind view as iChannel0 in NodeConfig
//!     }
//! }
//! ```

pub mod receiver;
pub use receiver::{NdiReceiver, NdiSource};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NdiError {
    #[error("NDI SDK not available — install from ndi.video and build with features = [\"ndi\"]")]
    SdkNotFound,

    #[error("NDI source '{name}' not found on network (waited {timeout_ms}ms)")]
    SourceNotFound { name: String, timeout_ms: u32 },

    #[error("NDI receive error: {0}")]
    ReceiveError(String),
}
