//! `scheng-output-ndi`
//!
//! NDI output sink for scheng. Cross-platform (macOS, Windows, Linux).
//!
//! NDI (Network Device Interface) by Vizrt sends video over a local network.
//! Discoverable by OBS (NDI input plugin), Resolume, vMix, and any NDI receiver.
//!
//! # Why NDI runs separately (from shadecore docs)
//!
//! "NDI requires a different runtime lifecycle, different threading assumptions,
//!  and tighter timing guarantees. Rather than complicate the core render loop,
//!  NDI runs in a dedicated execution mode."
//!
//! In scheng this means: NdiSink is a standard OutputSink but the host binary
//! should give it its own thread and timing budget. See NdiSink::new() docs.
//!
//! # Requirements
//!
//! - NDI SDK installed (https://ndi.video/download-ndi-sdk/)
//! - NDI SDK Rust bindings crate (or use the raw C FFI in ffi.rs)
//!
//! # Status
//!
//! Phase 3 implementation. The OutputSink interface and frame pipeline are
//! fully defined. The NDI SDK binding is stubbed — wire in your preferred
//! NDI Rust crate (ndi, ndi-sdk, or raw FFI).

pub mod config;
pub mod sink;

pub use config::NdiConfig;
pub use sink::NdiSink;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NdiError {
    #[error("NDI SDK not found — install the NDI SDK from ndi.video")]
    SdkNotFound,

    #[error("NDI sender creation failed for source '{name}'")]
    CreateFailed { name: String },

    #[error("NDI send error: {0}")]
    SendFailed(String),
}
