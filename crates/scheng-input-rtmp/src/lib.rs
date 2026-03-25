//! `scheng-input-rtmp`
//!
//! Pull a live RTMP or RTSP stream via an ffmpeg subprocess and upload
//! decoded frames to a wgpu RGBA texture each render cycle.
//!
//! Works with any URL ffmpeg can read:
//! - `rtmp://server/live/key`
//! - `rtmps://server/live/key`
//! - `rtsp://camera/stream`
//! - `srt://server:1234`
//! - Any other ffmpeg-supported ingest URL
//!
//! # Architecture
//!
//! ```text
//! ffmpeg subprocess
//!   -i <url> -f rawvideo -pix_fmt rgba pipe:1
//!     → stdout → bounded channel
//!
//! render thread
//!   RtmpReceiver::poll(&device, &queue)
//!     → drain channel → upload latest frame to wgpu texture
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use scheng_input_rtmp::RtmpReceiver;
//!
//! let mut recv = RtmpReceiver::open("rtmp://localhost/live/key", 1280, 720, &device, &queue)?;
//!
//! // In render loop:
//! if recv.poll(&device, &queue) {
//!     if let Some(view) = recv.texture_view() {
//!         // bind as iChannel0
//!     }
//! }
//! ```

pub mod receiver;
pub use receiver::RtmpReceiver;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RtmpError {
    #[error("ffmpeg not found — install ffmpeg")]
    FfmpegNotFound,

    #[error("Failed to spawn ffmpeg: {0}")]
    SpawnFailed(String),

    #[error("Stream ended or ffmpeg process exited")]
    StreamEnded,
}
