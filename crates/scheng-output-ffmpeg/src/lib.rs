//! `scheng-output-ffmpeg`
//!
//! FFmpeg output sink for scheng. Supports:
//! - RTSP / RTMP streaming (e.g. to mediamtx, nginx-rtmp)
//! - Local file recording (H.264, ProRes, etc.)
//!
//! # Architecture (matches shadecore's proven model)
//!
//! ```text
//! render thread
//!   execute_frame() → FfmpegSink::present()
//!     → readback pixels (Vec<u8> RGBA)
//!     → channel.try_send(frame)   ← non-blocking; drops frame if channel full
//!
//! worker thread (spawned at FfmpegSink::new)
//!   loop {
//!     frame = channel.recv()
//!     write frame bytes → ffmpeg stdin pipe
//!   }
//! ```
//!
//! Frame drops are preferred over stalling the render loop.
//! The bounded channel acts as a pressure-relief valve.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use scheng_output_ffmpeg::{FfmpegSink, FfmpegConfig, OutputTarget};
//!
//! let config = FfmpegConfig {
//!     width: 1280, height: 720, framerate: 30,
//!     target: OutputTarget::Rtsp { url: "rtsp://localhost:8554/live".into() },
//!     ..Default::default()
//! };
//!
//! let mut sink = FfmpegSink::new(config).expect("ffmpeg not found");
//! // Pass sink to WgpuRuntime::execute_frame(...)
//! ```

pub mod config;
pub mod sink;
pub mod worker;

pub use config::{FfmpegConfig, OutputTarget, RecordingConfig};
pub use sink::FfmpegSink;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FfmpegError {
    #[error("ffmpeg not found at '{path}' — install ffmpeg or set ffmpeg_path in config")]
    NotFound { path: String },

    #[error("ffmpeg process failed to start: {0}")]
    SpawnFailed(#[from] std::io::Error),

    #[error("Frame dimensions must be even for H.264/H.265 encoding (got {width}×{height})")]
    OddDimensions { width: u32, height: u32 },

    #[error("Worker thread panicked")]
    WorkerPanic,

    #[error("Config error: {0}")]
    Config(String),
}
