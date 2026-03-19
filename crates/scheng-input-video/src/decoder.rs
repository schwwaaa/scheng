//! `decoder.rs` — video file decoding via ffmpeg-next.
//!
//! `VideoDecoder` opens a video file, decodes frames, converts to RGBA8,
//! and uploads to a `VideoTexture` for use in the wgpu render pipeline.
//!
//! Frame selection: `FrameCtx::time` (seconds) → frame index via clip fps.
//! Looping: when time exceeds clip duration, wraps around.

use crate::{texture::VideoTexture, VideoError};

// ── Stub (feature = "decode" disabled) ───────────────────────────────────

#[cfg(not(feature = "decode"))]
pub struct VideoDecoder;

#[cfg(not(feature = "decode"))]
impl VideoDecoder {
    pub fn open(_path: &str, _device: &wgpu::Device, _queue: &wgpu::Queue)
        -> Result<Self, VideoError>
    {
        Err(VideoError::NotEnabled)
    }
    pub fn upload_frame(&mut self, _time_secs: f32, _queue: &wgpu::Queue) {}
    pub fn texture_view(&self) -> Option<wgpu::TextureView> { None }
    pub fn width(&self)    -> u32 { 0 }
    pub fn height(&self)   -> u32 { 0 }
    pub fn duration(&self) -> f32 { 0.0 }
    pub fn fps(&self)      -> f32 { 0.0 }
}

// ── Real decoder (feature = "decode") ────────────────────────────────────

#[cfg(feature = "decode")]
use ffmpeg_next as ffmpeg;

#[cfg(feature = "decode")]
pub struct VideoDecoder {
    /// Path stored for error messages and debug logging.
    path:        String,
    /// Video width in pixels.
    width:       u32,
    /// Video height in pixels.
    height:      u32,
    /// Clip duration in seconds.
    duration:    f32,
    /// Frames per second (used for time → frame index mapping).
    fps:         f32,
    /// Total frame count.
    frame_count: u64,
    /// The ffmpeg input context.
    ictx:        ffmpeg::format::context::Input,
    /// Index of the video stream within the container.
    stream_idx:  usize,
    /// The software scaler — converts decoded frames to RGBA8.
    scaler:      ffmpeg::software::scaling::Context,
    /// The video decoder codec context.
    decoder:     ffmpeg::codec::decoder::Video,
    /// Cached GPU texture — created once, reused each frame.
    video_tex:   VideoTexture,
    /// Last decoded frame index (avoids redundant seeks).
    last_frame:  i64,
}

#[cfg(feature = "decode")]
impl VideoDecoder {
    /// Open a video file and prepare for decoding.
    ///
    /// Creates the GPU texture at the video's native resolution.
    pub fn open(path: &str, device: &wgpu::Device, queue: &wgpu::Queue)
        -> Result<Self, VideoError>
    {
        ffmpeg::init().map_err(|e| VideoError::Open {
            path: path.into(), message: e.to_string()
        })?;

        let ictx = ffmpeg::format::input(&path)
            .map_err(|e| VideoError::Open { path: path.into(), message: e.to_string() })?;

        // Find the best video stream
        let stream = ictx.streams()
            .best(ffmpeg::media::Type::Video)
            .ok_or_else(|| VideoError::NoVideoStream { path: path.into() })?;

        let stream_idx = stream.index();
        let codec_ctx  = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
            .map_err(|e| VideoError::Open { path: path.into(), message: e.to_string() })?;
        let decoder = codec_ctx.decoder().video()
            .map_err(|e| VideoError::Open { path: path.into(), message: e.to_string() })?;

        let width  = decoder.width();
        let height = decoder.height();

        // Time base and fps
        let tb       = stream.time_base();
        let dur_pts  = stream.duration();
        let duration = if dur_pts > 0 {
            dur_pts as f32 * tb.numerator() as f32 / tb.denominator() as f32
        } else {
            // Fall back to container duration
            ictx.duration() as f32 / ffmpeg::ffi::AV_TIME_BASE as f32
        };

        let fps = {
            let r = stream.avg_frame_rate();
            r.numerator() as f32 / r.denominator().max(1) as f32
        };
        let frame_count = (duration * fps) as u64;

        // Software scaler: decoded pixel format → RGBA8
        let scaler = ffmpeg::software::scaling::Context::get(
            decoder.format(),
            width, height,
            ffmpeg::format::Pixel::RGBA,
            width, height,
            ffmpeg::software::scaling::flag::Flags::BILINEAR,
        ).map_err(|e| VideoError::Open { path: path.into(), message: e.to_string() })?;

        let video_tex = VideoTexture::new(device, width, height, "video_frame");

        // Upload a black frame so the texture is valid before the first decode
        let black = vec![0u8; (width * height * 4) as usize];
        video_tex.upload(queue, &black);

        log::info!("Video '{}': {}×{} {:.2}fps {:.1}s ({} frames)",
            path, width, height, fps, duration, frame_count);

        Ok(Self {
            path: path.into(),
            width, height, duration, fps, frame_count,
            ictx, stream_idx, scaler, decoder, video_tex,
            last_frame: -1,
        })
    }

    /// Upload the frame at `time_secs` to the GPU texture.
    ///
    /// Loops the clip if `time_secs` exceeds duration.
    /// Skips decoding if the frame index hasn't changed since last call.
    pub fn upload_frame(&mut self, time_secs: f32, queue: &wgpu::Queue) {
        if self.duration <= 0.0 || self.fps <= 0.0 { return; }

        // Loop time within clip duration
        let looped_time  = time_secs % self.duration;
        let target_frame = (looped_time * self.fps) as i64;

        // Skip if same frame
        if target_frame == self.last_frame { return; }

        // Seek if we need to go backwards or jump forward significantly
        if target_frame < self.last_frame || target_frame > self.last_frame + 30 {
            let timestamp = (looped_time as f64 * ffmpeg::ffi::AV_TIME_BASE as f64) as i64;
            if self.ictx.seek(timestamp, ..timestamp).is_err() {
                log::warn!("Video seek failed for '{}' at {:.2}s", self.path, time_secs);
            }
            self.decoder.flush();
        }

        // Decode forward to target frame
        let mut frame_idx = if target_frame < self.last_frame { 0i64 } else { self.last_frame };
        let mut decoded   = ffmpeg::frame::Video::empty();
        let mut rgba      = ffmpeg::frame::Video::empty();

        'outer: for (stream, packet) in self.ictx.packets() {
            if stream.index() != self.stream_idx { continue; }
            if self.decoder.send_packet(&packet).is_err() { continue; }

            while self.decoder.receive_frame(&mut decoded).is_ok() {
                frame_idx += 1;
                if frame_idx >= target_frame {
                    // Convert to RGBA
                    if self.scaler.run(&decoded, &mut rgba).is_ok() {
                        let pixels = rgba.data(0);
                        if pixels.len() == (self.width * self.height * 4) as usize {
                            self.video_tex.upload(queue, pixels);
                            self.last_frame = frame_idx;
                        }
                    }
                    break 'outer;
                }
            }
        }
    }

    /// A wgpu texture view ready to bind as iChannel0.
    pub fn texture_view(&self) -> Option<wgpu::TextureView> {
        Some(self.video_tex.view())
    }

    pub fn width(&self)    -> u32 { self.width }
    pub fn height(&self)   -> u32 { self.height }
    pub fn duration(&self) -> f32 { self.duration }
    pub fn fps(&self)      -> f32 { self.fps }
}
