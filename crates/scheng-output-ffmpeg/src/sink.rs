//! `sink.rs` — FfmpegSink: implements OutputSink from scheng-runtime-wgpu.

use scheng_graph::NodeId;
use scheng_runtime_wgpu::{executor::OutputSink, FrameCtx, RenderTarget};

use crate::{worker::{FfmpegWorker, RawFrame}, FfmpegConfig, FfmpegError};

/// OutputSink that encodes frames with ffmpeg.
///
/// Spawns a background ffmpeg process on `new()`.
/// Sends RGBA pixel data to it via a bounded channel on each `present()` call.
/// Stops and waits for ffmpeg on `drop()`.
///
/// # Example — RTSP streaming
///
/// ```rust,no_run
/// use scheng_output_ffmpeg::{FfmpegSink, FfmpegConfig, OutputTarget};
///
/// let config = FfmpegConfig {
///     width: 1280, height: 720, framerate: 30,
///     target: OutputTarget::Rtsp { url: "rtsp://localhost:8554/live".into() },
///     ..Default::default()
/// };
/// let mut sink = FfmpegSink::new(config).unwrap();
/// ```
///
/// # Example — local file recording
///
/// ```rust,no_run
/// use scheng_output_ffmpeg::{FfmpegSink, FfmpegConfig, OutputTarget, EncodingConfig};
///
/// let config = FfmpegConfig {
///     width: 1280, height: 720, framerate: 30,
///     target: OutputTarget::File { path: "output.mp4".into(), overwrite: true },
///     encoding: EncodingConfig {
///         codec:  "libx264".into(),
///         preset: "fast".into(),
///         bitrate: "8M".into(),
///         tune_zerolatency: false,
///         ..Default::default()
///     },
///     ..Default::default()
/// };
/// let mut sink = FfmpegSink::new(config).unwrap();
/// ```
pub struct FfmpegSink {
    worker: FfmpegWorker,
    width:  u32,
    height: u32,
}

impl FfmpegSink {
    /// Create a new FfmpegSink. Spawns the ffmpeg process immediately.
    ///
    /// Fails if ffmpeg is not installed or the config is invalid.
    pub fn new(config: FfmpegConfig) -> Result<Self, FfmpegError> {
        let width  = config.width;
        let height = config.height;
        let worker = FfmpegWorker::start(&config)?;
        Ok(Self { worker, width, height })
    }

    /// Stop the ffmpeg process and wait for it to finish.
    ///
    /// Called automatically on drop, but can be called explicitly to
    /// check for encoding errors or to cleanly finalize an MP4.
    pub fn stop(&mut self) {
        self.worker.stop();
    }

    pub fn frames_sent(&self)    -> u64 { self.worker.frames_sent() }
    pub fn frames_dropped(&self) -> u64 { self.worker.frames_dropped() }
}

impl OutputSink for FfmpegSink {
    /// Read back rendered pixels and enqueue them for ffmpeg.
    ///
    /// This is non-blocking — if the worker thread is behind, the frame
    /// is dropped rather than stalling the render loop.
    fn present(
        &mut self,
        _node_id: NodeId,
        target:   &RenderTarget,
        ctx:      &FrameCtx,
        device:   &wgpu::Device,
        queue:    &wgpu::Queue,
    ) {
        // Skip if resolution doesn't match config.
        // (Resolution changes trigger render target reallocation upstream.)
        if ctx.width != self.width || ctx.height != self.height {
            log::warn!(
                "FfmpegSink: frame {}×{} doesn't match config {}×{} — skipping",
                ctx.width, ctx.height, self.width, self.height
            );
            return;
        }

        // Readback is synchronous — GPU must have submitted work before this.
        // The executor calls present() after queue.submit(), so this is safe.
        let pixels = target.readback(device, queue);

        self.worker.send_frame(RawFrame {
            pixels,
            width:  self.width,
            height: self.height,
        });
    }
}
