//! `scheng-output-syphon`
//!
//! Syphon Metal output sink for scheng. macOS only.
//!
//! Publishes rendered frames to a Syphon server, making them available
//! to OBS (Syphon input), Resolume, VDMX, TouchDesigner, and any other
//! Syphon-compatible application on the same Mac.
//!
//! # Requirements
//!
//! - macOS only
//! - `vendor/Syphon.framework` at workspace root
//!   (download from github.com/Syphon/Syphon-Framework/releases)
//! - Xcode Command Line Tools
//!
//! # Quick start
//!
//! ```rust,no_run
//! #[cfg(target_os = "macos")]
//! {
//!     use scheng_output_syphon::SyphonSink;
//!     let mut sink = SyphonSink::new("scheng", mtl_device_ptr).unwrap();
//!     // Pass to WgpuRuntime::execute_frame(...)
//! }
//! ```

#[cfg(target_os = "macos")]
pub mod ffi;
#[cfg(target_os = "macos")]
pub mod sink;

#[cfg(target_os = "macos")]
pub use sink::SyphonSink;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyphonError {
    #[error("Syphon server creation failed — is Syphon.framework present?")]
    CreateFailed,

    #[error("Not on macOS — Syphon is a macOS-only technology")]
    NotMacOs,
}
