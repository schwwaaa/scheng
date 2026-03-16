//! `commands.rs` — Tauri IPC commands exposed to the WebView.
//!
//! All commands are called via `invoke("command_name", args)` from JavaScript.
//! They operate on `AppState` via `tauri::State<AppState>`.
//!
//! # JavaScript usage
//!
//! ```typescript
//! import { invoke } from '@tauri-apps/api/core';
//!
//! // Set a parameter (e.g. from a slider)
//! await invoke('set_param', { name: 'u_brightness', value: 0.75 });
//!
//! // Switch output mode
//! await invoke('set_output_mode', { mode: 'syphon' });
//!
//! // Get all params for building the UI
//! const params = await invoke('get_params');
//!
//! // Get engine status
//! const status = await invoke('get_engine_status');
//! ```

use serde::Deserialize;
use tauri::State;

use crate::engine::{AppState, EngineStatus, OutputMode};

// ── get_params ────────────────────────────────────────────────────────────

/// Returns the complete parameter schema for building the instrument UI.
///
/// Call once on startup to know what sliders/knobs to render.
/// Re-call after a params.json hot-reload (listen for "params-reloaded" event).
#[tauri::command]
pub fn get_params(state: State<AppState>) -> serde_json::Value {
    let store = state.param_store.lock().unwrap();
    serde_json::to_value(store.schema()).unwrap_or(serde_json::json!({"params":[]}))
}

// ── set_param ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SetParamArgs {
    pub name:  String,
    pub value: f32,
}

/// Set a parameter value from the UI (e.g. a slider move).
///
/// Value is clamped to the param's [min, max] range server-side.
/// Takes effect on the next render frame.
#[tauri::command]
pub fn set_param(args: SetParamArgs, state: State<AppState>) -> Result<(), String> {
    let mut store = state.param_store.lock().unwrap();
    store.set_by_name(&args.name, args.value)
        .map_err(|e| e.to_string())
}

// ── set_output_mode ───────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SetOutputModeArgs {
    pub mode: OutputMode,
}

/// Switch the output destination. Takes effect on the next frame.
///
/// Modes: "preview", "syphon" (macOS), "spout" (Windows), "stream", "ndi", "record"
#[tauri::command]
pub fn set_output_mode(args: SetOutputModeArgs, state: State<AppState>) -> Result<(), String> {
    let mut config = state.render_config.lock().unwrap();
    log::info!("Output mode → {:?}", args.mode);
    config.output_mode = args.mode;
    Ok(())
}

// ── start_recording / stop_recording ─────────────────────────────────────

#[derive(Deserialize)]
pub struct StartRecordingArgs {
    /// Output file path. Default: "recording.mp4"
    pub path:    Option<String>,
    /// Codec. Default: "libx264"
    pub codec:   Option<String>,
    /// Bitrate. Default: "8M"
    pub bitrate: Option<String>,
}

/// Begin recording to a local file.
///
/// Spawns an FFmpeg process in the render thread on the next frame.
#[tauri::command]
pub fn start_recording(args: StartRecordingArgs, state: State<AppState>) -> Result<(), String> {
    let mut config = state.render_config.lock().unwrap();
    if config.is_recording {
        return Err("Already recording".into());
    }
    if let Some(path) = args.path {
        config.record_path = path;
    }
    config.is_recording  = true;
    config.output_mode   = OutputMode::Record;
    log::info!("Recording started → {}", config.record_path);
    Ok(())
}

/// Stop recording. Waits for FFmpeg to flush and finalize the file.
#[tauri::command]
pub fn stop_recording(state: State<AppState>) -> Result<(), String> {
    let mut config = state.render_config.lock().unwrap();
    if !config.is_recording {
        return Err("Not recording".into());
    }
    config.is_recording = false;
    config.output_mode  = OutputMode::Preview;
    log::info!("Recording stopped");
    Ok(())
}

// ── get_engine_status ─────────────────────────────────────────────────────

/// Returns current engine status: frame count, output mode, GPU adapter name.
///
/// Poll this from the UI at 1–2Hz for status display, not every frame.
#[tauri::command]
pub fn get_engine_status(state: State<AppState>) -> EngineStatus {
    let frame        = *state.frame_count.lock().unwrap();
    let config       = state.render_config.lock().unwrap();
    let adapter_name = state.adapter_name.lock().unwrap().clone();
    let gpu_ready    = *state.gpu_ready.lock().unwrap();

    EngineStatus {
        running:      gpu_ready,
        frame,
        output_mode:  config.output_mode.clone(),
        is_recording: config.is_recording,
        adapter_name,
    }
}

// ── load_graph_json ───────────────────────────────────────────────────────

/// Load a graph from a JSON string (patch file format).
///
/// Graph JSON format:
/// ```json
/// {
///   "nodes": [
///     {"id": "src",  "kind": "ShaderSource"},
///     {"id": "pass", "kind": "ShaderPass"},
///     {"id": "out",  "kind": "PixelsOut"}
///   ],
///   "edges": [
///     {"from": "src",  "from_port": "out", "to": "pass", "to_port": "in"},
///     {"from": "pass", "from_port": "out", "to": "out",  "to_port": "in"}
///   ],
///   "shaders": {
///     "src":  "#version 330 core\n...",
///     "pass": "#version 330 core\n..."
///   }
/// }
/// ```
///
/// Phase 5: stub — returns Ok so the frontend can proceed. Full graph
/// deserialization lands when the JSON patch format is finalised in Phase 7.
#[tauri::command]
pub fn load_graph_json(_json: String, _state: State<AppState>) -> Result<(), String> {
    log::info!("load_graph_json: received (Phase 7 stub — graph switching not yet implemented)");
    Ok(())
}
