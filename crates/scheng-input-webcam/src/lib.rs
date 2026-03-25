//! `scheng-input-webcam`
//!
//! Webcam capture → wgpu RGBA texture for scheng instruments.
//!
//! Gated behind `features = ["native"]` — builds cleanly on all platforms
//! without it, returns `WebcamError::NotEnabled` at runtime.
//!
//! # Usage
//!
//! ```rust,ignore
//! // Build with: cargo build --features native
//! use scheng_input_webcam::Webcam;
//!
//! let mut cam = Webcam::open(0, 1280, 720, &device, &queue)?;
//!
//! // In render loop — polls for new frame, uploads if available
//! cam.poll(&queue);
//!
//! // Bind to iChannel0 in NodeConfig
//! let view = cam.texture_view();
//! ```

pub mod webcam;
pub use webcam::Webcam;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum WebcamError {
    #[error("Webcam capture not enabled — build with --features native")]
    NotEnabled,

    #[error("No camera found at index {0}")]
    NotFound(u32),

    #[error("Camera open failed: {0}")]
    OpenFailed(String),

    #[error("Frame capture failed: {0}")]
    CaptureFailed(String),
}
