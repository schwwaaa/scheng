//! `engine.rs` — AppState: shared mutable state between IPC commands and render thread.
//!
//! # Threading model
//!
//! `AppState` is wrapped in `Arc` and cloned between the Tauri command thread
//! and the render thread. Each inner field uses its own `Mutex` for fine-grained locking.
//!
//! Lock contention is designed to be minimal:
//! - `param_store` is locked by commands for <1µs (single f32 write)
//! - `render_config` is locked by commands only on mode changes (rare)
//! - The render thread reads `param_store` every frame but holds the lock only for
//!   `step_frame()` duration (~microseconds)

use std::sync::{Arc, Mutex};
use scheng_param_store::ParamStore;
use serde::{Deserialize, Serialize};

/// The complete output mode selection.
///
/// Matches shadecore's hotkey model:
/// 1 = preview only, 2 = syphon, 3 = spout, 4 = stream, 6 = ndi
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    /// Local preview only (no external output). Lowest overhead.
    Preview,
    /// Syphon Metal server (macOS only).
    Syphon,
    /// Spout2 sender (Windows only).
    Spout,
    /// FFmpeg RTSP/RTMP stream.
    Stream,
    /// NDI sender.
    Ndi,
    /// Local file recording via FFmpeg.
    Record,
}

impl Default for OutputMode {
    fn default() -> Self { Self::Preview }
}

/// Configuration that the render thread reads to decide where frames go.
#[derive(Debug, Clone, Default)]
pub struct RenderConfig {
    pub output_mode:  OutputMode,
    pub is_recording: bool,
    pub stream_url:   String,
    pub record_path:  String,
    /// Width × height for the render target.
    pub width:  u32,
    pub height: u32,
}

impl RenderConfig {
    pub fn new() -> Self {
        Self {
            width:  1280,
            height: 720,
            stream_url:  "rtsp://localhost:8554/live".into(),
            record_path: "recording.mp4".into(),
            ..Default::default()
        }
    }
}

/// Engine status snapshot returned by `get_engine_status`.
#[derive(Debug, Clone, Serialize)]
pub struct EngineStatus {
    pub running:        bool,
    pub frame:          u64,
    pub output_mode:    OutputMode,
    pub is_recording:   bool,
    pub adapter_name:   String,
}

/// Shared state owned by Tauri, cloned into the render thread.
///
/// Cheap to clone — all inner state is behind Arc.
#[derive(Clone)]
pub struct AppState {
    /// Live parameter values. Written by MIDI/OSC/IPC; read each frame.
    pub param_store:  Arc<Mutex<ParamStore>>,
    /// Output mode, dimensions, stream URL, recording state.
    pub render_config: Arc<Mutex<RenderConfig>>,
    /// Current frame counter (render thread writes, IPC reads).
    pub frame_count:  Arc<Mutex<u64>>,
    /// GPU adapter name (set by render thread after init).
    pub adapter_name: Arc<Mutex<String>>,
    /// True once the render thread has initialised the GPU.
    pub gpu_ready:    Arc<Mutex<bool>>,
}

impl AppState {
    pub fn new() -> Self {
        // Load params.json if present; fall back to empty store.
        let store = ParamStore::from_json_file("assets/params.json")
            .unwrap_or_else(|e| {
                log::warn!("No assets/params.json: {e} — using empty param store");
                ParamStore::empty()
            });

        Self {
            param_store:   Arc::new(Mutex::new(store)),
            render_config: Arc::new(Mutex::new(RenderConfig::new())),
            frame_count:   Arc::new(Mutex::new(0)),
            adapter_name:  Arc::new(Mutex::new("initialising…".into())),
            gpu_ready:     Arc::new(Mutex::new(false)),
        }
    }
}
