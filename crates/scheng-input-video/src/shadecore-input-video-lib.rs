use serde::{Deserialize, Serialize};
use std::{
    ffi::OsStr,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>, // RGBA, row-major, tightly packed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoConfig {
    /// Output width (pixels).
    #[serde(default = "default_width")]
    pub width: u32,

    /// Output height (pixels).
    #[serde(default = "default_height")]
    pub height: u32,

    /// Target output fps (currently not enforced when trusting source fps,
    /// but kept for schema compatibility and future use if needed).
    #[serde(default = "default_fps")]
    pub fps: u32,

    /// Input file path.
    pub file: String,

    /// Whether to loop the video.
    #[serde(default = "default_loop", rename = "loop")]
    pub r#loop: bool,

    /// Optional explicit ffmpeg binary path.
    #[serde(default)]
    pub ffmpeg_path: Option<String>,
}

fn default_width() -> u32 {
    640
}
fn default_height() -> u32 {
    360
}
fn default_fps() -> u32 {
    30
}
fn default_loop() -> bool {
    true
}

#[derive(thiserror::Error, Debug)]
pub enum VideoError {
    #[error(
        "ffmpeg not found (set scheng_FFMPEG, config.ffmpeg_path, or ensure bundled ffmpeg exists)"
    )]
    FfmpegNotFound,

    #[error("failed to spawn ffmpeg: {0}")]
    Spawn(#[from] io::Error),

    #[error("ffmpeg exited early")]
    FfmpegExited,

    #[error("no frame available yet")]
    NoFrameYet,

    #[error("invalid config: {0}")]
    InvalidConfig(String),
}

pub struct VideoDecoder {
    cfg: VideoConfig,
    latest: Arc<Mutex<Option<VideoFrame>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl VideoDecoder {
    pub fn from_config(cfg: VideoConfig) -> Result<Self, VideoError> {
        if cfg.file.trim().is_empty() {
            return Err(VideoError::InvalidConfig("file is empty".into()));
        }
        if cfg.width == 0 || cfg.height == 0 {
            return Err(VideoError::InvalidConfig(
                "width/height must be > 0".into(),
            ));
        }
        if cfg.fps == 0 {
            return Err(VideoError::InvalidConfig("fps must be > 0".into()));
        }

        let latest = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));

        let cfg_for_thread = cfg.clone();
        let latest_for_thread = Arc::clone(&latest);
        let stop_for_thread = Arc::clone(&stop);

        let worker = thread::spawn(move || {
            decode_loop(cfg_for_thread, latest_for_thread, stop_for_thread);
        });

        Ok(Self {
            cfg,
            latest,
            stop,
            worker: Some(worker),
        })
    }

    pub fn from_json_path(path: impl AsRef<Path>) -> Result<Self, VideoError> {
        let text = std::fs::read_to_string(path.as_ref())
            .map_err(|e| VideoError::InvalidConfig(format!("read json: {e}")))?;
        let cfg: VideoConfig = serde_json::from_str(&text)
            .map_err(|e| VideoError::InvalidConfig(format!("parse json: {e}")))?;
        Self::from_config(cfg)
    }

    pub fn config(&self) -> &VideoConfig {
        &self.cfg
    }

    /// Non-blocking: returns the latest available frame (if any), otherwise NoFrameYet.
    pub fn poll_rgba(&mut self) -> Result<VideoFrame, VideoError> {
        let guard = self.latest.lock().unwrap();
        if let Some(f) = guard.as_ref() {
            Ok(f.clone())
        } else {
            Err(VideoError::NoFrameYet)
        }
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }
}

// ---------------- internal ----------------

fn decode_loop(cfg: VideoConfig, latest: Arc<Mutex<Option<VideoFrame>>>, stop: Arc<AtomicBool>) {
    let frame_len = (cfg.width as usize) * (cfg.height as usize) * 4;
    let mut buf = vec![0u8; frame_len];

    // Ensure we never silently swallow ffmpeg spawn failures.
    let mut logged_spawn_error = false;

    while !stop.load(Ordering::SeqCst) {
        let ffmpeg = resolve_ffmpeg_path(cfg.ffmpeg_path.as_deref())
            .unwrap_or_else(|| PathBuf::from("ffmpeg"));

        let mut child = match spawn_ffmpeg(&ffmpeg, &cfg) {
            Ok(c) => {
                // Once we successfully spawn, clear any previous error flag.
                logged_spawn_error = false;
                c
            }
            Err(e) => {
                if !logged_spawn_error {
                    eprintln!(
                        "scheng-input-video: failed to spawn ffmpeg at {:?}: {}",
                        ffmpeg, e
                    );
                    logged_spawn_error = true;
                }

                // If looping is disabled, fail fast instead of silently spinning.
                if !cfg.r#loop {
                    return;
                }

                // Backoff a bit before retrying, to avoid busy-looping.
                thread::sleep(Duration::from_millis(500));
                continue;
            }
        };

        let mut stdout = child.stdout.take().expect("ffmpeg stdout piped");

        loop {
            if stop.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                return;
            }

            match stdout.read_exact(&mut buf) {
                Ok(()) => {
                    let frame = VideoFrame {
                        width: cfg.width,
                        height: cfg.height,
                        bytes: buf.clone(),
                    };
                    *latest.lock().unwrap() = Some(frame);
                }
                Err(e) => {
                    // EOF or stream ended. Decide whether to loop.
                    let _ = child.kill();
                    let _ = child.wait();

                    if cfg.r#loop {
                        // For looped playback, break out to respawn ffmpeg.
                        break;
                    } else {
                        let _ = e;
                        // Leave the last frame in `latest` and exit the worker.
                        return;
                    }
                }
            }
        }
    }
}

/// Spawn ffmpeg configured to:
/// - read the input at (approx) real-time speed (`-re`), trusting source timestamps/fps
/// - scale to cfg.width x cfg.height
/// - flip vertically, so the resulting RGBA is GL-friendly (bottom-left origin in UVs)
fn spawn_ffmpeg(ffmpeg: &Path, cfg: &VideoConfig) -> io::Result<Child> {
    // ffmpeg args:
    // -re                 (throttle to real time using input timestamps)
    // -loglevel error     (quiet)
    // -stream_loop -1     (optional, for looping)
    // -vf scale=WxH,vflip (trust source fps; no fps= filter)
    // -pix_fmt rgba -f rawvideo pipe:1
    let mut cmd = Command::new(ffmpeg);

    cmd.arg("-hide_banner").arg("-loglevel").arg("error");

    // Throttle decoding so frames come out at (approx) real-time rate based on input timestamps.
    cmd.arg("-re");

    if cfg.r#loop {
        // For many demuxers, stream_loop works well for local files.
        cmd.arg("-stream_loop").arg("-1");
    }

    cmd.arg("-i")
        .arg(&cfg.file)
        .arg("-vf")
        .arg(format!("scale={}:{},vflip", cfg.width, cfg.height))
        .arg("-pix_fmt")
        .arg("rgba")
        .arg("-f")
        .arg("rawvideo")
        .arg("pipe:1")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    cmd.spawn()
}

fn resolve_ffmpeg_path(explicit: Option<&str>) -> Option<PathBuf> {
    // Priority:
    // 1) explicit config path
    // 2) scheng_FFMPEG env var
    // 3) bundled ffmpeg near executable (vendor/ffmpeg/ffmpeg)
    // 4) workspace dev path (schengine/vendor/ffmpeg/ffmpeg)
    if let Some(p) = explicit {
        return Some(PathBuf::from(p));
    }

    if let Some(p) = std::env::var_os("scheng_FFMPEG") {
        return Some(PathBuf::from(p));
    }

    // bundled: next to binary: <exe>/../vendor/ffmpeg/ffmpeg (or ffmpeg.exe on windows)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let candidate = exe_dir
                .join("..")
                .join("vendor")
                .join("ffmpeg")
                .join(ffmpeg_filename());
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    // dev: workspace root vendor/ffmpeg/ffmpeg
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR")?);
    // crates/scheng-input-video -> workspace root is ../..
    let workspace_root = manifest_dir.parent()?.parent()?.to_path_buf();
    let candidate = workspace_root.join("vendor").join("ffmpeg").join(ffmpeg_filename());
    if candidate.exists() {
        return Some(candidate);
    }

    None
}

fn ffmpeg_filename() -> &'static OsStr {
    #[cfg(windows)]
    {
        OsStr::new("ffmpeg.exe")
    }
    #[cfg(not(windows))]
    {
        OsStr::new("ffmpeg")
    }
}
