//! `scheng-input-video`
//!
//! Video file decoder → wgpu RGBA texture for scheng `VideoDecodeSource` nodes.
//!
//! # Design
//!
//! `VideoDecoder` opens a video file, decodes frames on demand, and uploads
//! them to a wgpu texture. The texture is then passed to the graph as the
//! `VideoDecodeSource` node's output — downstream nodes sample it via iChannel0.
//!
//! Frame selection is time-based: `FrameCtx::time` (seconds) maps to a frame
//! index using the clip's nominal fps. This matches shadecore's model exactly.
//!
//! # Usage
//!
//! ```rust,ignore
//! use scheng_input_video::VideoDecoder;
//!
//! // Open a video file
//! let mut decoder = VideoDecoder::open("assets/clip.mp4", &device, &queue)?;
//!
//! // Each frame: upload the frame at the current time position
//! decoder.upload_frame(ctx.time, &queue);
//!
//! // The texture is available for binding
//! let texture_view = decoder.texture_view();
//! ```
//!
//! # Feature flags
//!
//! - `decode` (default): enables ffmpeg-next decoding
//! - Without `decode`: `VideoDecoder::open` returns `VideoError::NotEnabled`

pub mod decoder;
pub mod texture;
pub mod node;

pub use decoder::VideoDecoder;
pub use texture::VideoTexture;
pub use node::{VideoSourceManager, VideoInfo};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum VideoError {
    #[error("Video decoding not enabled — build with --features decode")]
    NotEnabled,

    #[error("Failed to open video file '{path}': {message}")]
    Open { path: String, message: String },

    #[error("No video stream found in '{path}'")]
    NoVideoStream { path: String },

    #[error("Failed to decode frame: {0}")]
    Decode(String),

    #[error("Unsupported pixel format — expected RGBA or convertible")]
    UnsupportedFormat,
}
