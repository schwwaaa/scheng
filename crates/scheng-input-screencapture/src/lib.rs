//! `scheng-input-screencapture`
//!
//! Captures the primary display each frame and uploads it as a wgpu RGBA
//! texture, following the same `InputSource` plugin contract as
//! `scheng-input-webcam` and `scheng-input-ndi`.
//!
//! # Platform support
//!
//! | Platform | Backend                        | Notes                         |
//! |----------|-------------------------------|-------------------------------|
//! | macOS    | CoreGraphics CGDisplayCreateImage | Works without user permission for own screen on macOS < 14. macOS 14+ may require Screen Recording permission. |
//! | Windows  | `screenshots` crate (DXGI)     | Requires build tools           |
//! | Linux    | Stub — not yet implemented     |                               |
//!
//! # Usage
//!
//! ```rust,ignore
//! use scheng_input_screencapture::ScreenCapture;
//!
//! let mut cap = ScreenCapture::new(&device, &queue)?;
//!
//! // Each frame — captures the primary display and uploads to GPU
//! cap.poll(&device, &queue);
//!
//! // Inject into a node's iChannel
//! node_config.input_textures[0] = cap.texture_arc();
//! ```

pub mod capturer;
pub use capturer::ScreenCapture;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScreenCaptureError {
    #[error("Screen capture is not supported on this platform")]
    Unsupported,

    #[error("Screen capture initialisation failed: {0}")]
    InitFailed(String),

    #[error("Frame capture failed: {0}")]
    CaptureFailed(String),
}
