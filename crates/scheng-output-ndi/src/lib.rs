//! `scheng-output-ndi` — NDI output sink for scheng.
//!
//! Broadcasts rendered frames as an NDI source discoverable by OBS (NDI
//! input plugin), Resolume, vMix, and any NDI receiver on the network.
//!
//! # Requirements
//! Install NDI 6 SDK: https://ndi.video/download-ndi-sdk/
//! macOS: /Library/NDI SDK for Apple  (or set NDI_SDK_DIR)
//!
//! # Enabling
//! ```toml
//! scheng-output-ndi = { path = "...", features = ["ndi"] }
//! ```
//! Then in main.rs:
//! ```rust,ignore
//! #[cfg(feature = "ndi")]
//! let ndi_sink = NdiSink::new(NdiConfig::default()).unwrap();
//! ```

pub mod config;
pub mod sink;

pub use config::NdiConfig;
pub use sink::NdiSink;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NdiError {
    #[error("NDI SDK not found — install from ndi.video and set NDI_SDK_DIR")]
    SdkNotFound,

    #[error("NDI sender creation failed for source '{name}'")]
    CreateFailed { name: String },

    #[error("NDI send error: {0}")]
    SendFailed(String),
}
