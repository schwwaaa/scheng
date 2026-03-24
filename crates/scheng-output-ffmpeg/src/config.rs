//! `config.rs` — FfmpegConfig and related types.
//!
//! Matches shadecore's `assets/output.json` schema so existing
//! shadecore configs can be used without changes.

use serde::{Deserialize, Serialize};

/// The complete FFmpeg output configuration.
///
/// Load from JSON with `FfmpegConfig::from_json_file(path)` or
/// construct directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FfmpegConfig {
    /// Path to the ffmpeg binary. Defaults to `"ffmpeg"` (must be in PATH).
    pub ffmpeg_path: String,

    /// Output width in pixels. Must be even for H.264/H.265.
    pub width: u32,

    /// Output height in pixels. Must be even for H.264/H.265.
    pub height: u32,

    /// Output framerate (frames per second).
    pub framerate: u32,

    /// Where the encoded output goes.
    pub target: OutputTarget,

    /// Encoding settings.
    pub encoding: EncodingConfig,

    /// How many frames to buffer in the channel before dropping.
    /// Higher = smoother under load; lower = lower latency.
    /// Default: 4 (from shadecore).
    pub queue_depth: usize,
}

impl Default for FfmpegConfig {
    fn default() -> Self {
        Self {
            ffmpeg_path: "ffmpeg".into(),
            width:       1280,
            height:      720,
            framerate:   30,
            target:      OutputTarget::default(),
            encoding:    EncodingConfig::default(),
            queue_depth: 4,
        }
    }
}

impl FfmpegConfig {
    /// Load from a JSON file (matches shadecore output.json schema).
    pub fn from_json_file(path: &str) -> Result<Self, crate::FfmpegError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| crate::FfmpegError::Config(format!("Cannot read {path}: {e}")))?;

        // Support both a top-level FfmpegConfig and shadecore's nested output.json
        // by trying both parse shapes.
        serde_json::from_str::<Self>(&text)
            .or_else(|_| {
                // shadecore's output.json has a "stream" sub-object
                let v: serde_json::Value = serde_json::from_str(&text)
                    .map_err(|e| crate::FfmpegError::Config(e.to_string()))?;
                // Extract stream config if present
                if let Some(stream) = v.get("stream") {
                    serde_json::from_value::<Self>(stream.clone())
                        .map_err(|e| crate::FfmpegError::Config(e.to_string()))
                } else {
                    Err(crate::FfmpegError::Config("No valid config found in JSON".into()))
                }
            })
            .map_err(|e| crate::FfmpegError::Config(e.to_string()))
    }

    /// Validate dimensions are even (required by most video codecs).
    pub fn validate(&self) -> Result<(), crate::FfmpegError> {
        if self.width % 2 != 0 || self.height % 2 != 0 {
            return Err(crate::FfmpegError::OddDimensions {
                width: self.width,
                height: self.height,
            });
        }
        Ok(())
    }

    /// Build the ffmpeg argument list for this config.
    /// Input is always stdin (`pipe:0`) with raw RGBA frames.
    pub fn build_args(&self) -> Vec<String> {
        let mut args = vec![
            // Input: raw RGBA from stdin
            "-f".into(), "rawvideo".into(),
            "-pixel_format".into(), "rgba".into(),
            "-video_size".into(), format!("{}x{}", self.width, self.height),
            "-framerate".into(), self.framerate.to_string(),
            "-i".into(), "pipe:0".into(),
            // Suppress banner and stats
            "-nostats".into(),
            "-hide_banner".into(),
        ];

        // No audio input — suppress audio stream
        args.push("-an".into());

        // Encoding
        args.extend([
            "-c:v".into(), self.encoding.codec.clone(),
        ]);

        if !self.encoding.preset.is_empty() {
            args.extend(["-preset".into(), self.encoding.preset.clone()]);
        }

        if !self.encoding.bitrate.is_empty() {
            args.extend(["-b:v".into(), self.encoding.bitrate.clone()]);
        }

        args.extend(["-pix_fmt".into(), self.encoding.pixel_format.clone()]);

        // Colorspace metadata — critical for correct colors in players/broadcast.
        // bt709 = HD standard (Rec.709), full range for maximum fidelity.
        // Without these flags ffmpeg defaults to bt601 (SD) which causes
        // washed-out / shifted colors on HD content.
        args.extend([
            "-colorspace".into(), "bt709".into(),
            "-color_primaries".into(), "bt709".into(),
            "-color_trc".into(), "bt709".into(),
            "-color_range".into(), "tv".into(),  // 16-235 broadcast range
        ]);

        if self.encoding.tune_zerolatency {
            args.extend(["-tune".into(), "zerolatency".into()]);
        }

        // Output target
        match &self.target {
            OutputTarget::Rtsp { url } => {
                // Push to RTSP server (e.g. MediaMTX). Requires server running at URL.
                args.extend([
                    "-f".into(), "rtsp".into(),
                    "-rtsp_transport".into(), "tcp".into(),
                    "-muxdelay".into(), "0.1".into(),
                    url.clone(),
                ]);
            }
            OutputTarget::Rtmp { url } => {
                // Push to RTMP ingest (OBS, nginx-rtmp, YouTube, Twitch etc.)
                // Most platforms expect FLV container over RTMP.
                args.extend(["-f".into(), "flv".into(), url.clone()]);
            }
            OutputTarget::File { path, overwrite } => {
                if *overwrite {
                    args.push("-y".into());
                }
                args.push(path.clone());
            }
        }

        args
    }
}

/// Where the encoded output is sent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum OutputTarget {
    /// RTSP stream (e.g. `rtsp://localhost:8554/live` via mediamtx).
    Rtsp { url: String },
    /// RTMP stream (e.g. `rtmp://server/live/key` for OBS ingest).
    Rtmp { url: String },
    /// Local file recording.
    File { path: String, #[serde(default = "default_true")] overwrite: bool },
}

impl Default for OutputTarget {
    fn default() -> Self {
        Self::Rtsp { url: "rtsp://localhost:8554/live".into() }
    }
}

/// Encoding settings for the output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EncodingConfig {
    /// Video codec. Default: `libx264`.
    pub codec: String,
    /// Encoding preset (speed vs compression trade-off).
    /// Default: `ultrafast` for streaming, `fast` for recording.
    pub preset: String,
    /// Target bitrate. Default: `"4M"`.
    pub bitrate: String,
    /// Output pixel format. Default: `yuv420p` (required by most decoders).
    pub pixel_format: String,
    /// Add `-tune zerolatency` for streaming. Default: true.
    pub tune_zerolatency: bool,
}

impl Default for EncodingConfig {
    fn default() -> Self {
        Self {
            codec:             "libx264".into(),
            preset:            "veryfast".into(),
            bitrate:           "4M".into(),
            pixel_format:      "yuv420p".into(),
            tune_zerolatency:  true,
        }
    }
}

/// Separate recording config (used when recording alongside streaming).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RecordingConfig {
    pub ffmpeg_path: String,
    pub output_path: String,
    pub codec:       String,
    pub preset:      String,
    pub bitrate:     String,
    pub framerate:   u32,
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            ffmpeg_path: "ffmpeg".into(),
            output_path: "recording.mp4".into(),
            codec:       "libx264".into(),
            preset:      "fast".into(),
            bitrate:     "8M".into(),
            framerate:   30,
        }
    }
}

fn default_true() -> bool { true }
