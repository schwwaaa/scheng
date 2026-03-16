//! `render_loop.rs` — the dedicated render thread that owns WgpuRuntime.
//!
//! This thread is spawned in `lib.rs::setup()` and runs for the app's lifetime.
//!
//! # Per-frame steps
//!
//! 1. OSC: drain UDP socket (non-blocking)
//! 2. Hot-reload: check for file changes
//! 3. Params: step_frame() — advance smoothed values toward targets
//! 4. NodeConfig: build from current param values
//! 5. GPU: execute_frame() → OutputSink
//! 6. Preview: every PREVIEW_INTERVAL frames, readback → JPEG → emit
//! 7. Timing: sleep to target framerate
//!
//! # Threading invariant
//!
//! WgpuRuntime is owned exclusively by this thread.
//! No GPU calls happen on the Tauri command threads.

use std::time::{Duration, Instant};

use scheng_graph::{Graph, NodeKind};
use scheng_runtime_wgpu::{
    executor::PixelReadbackSink,
    FrameCtx, WgpuRuntime,
};
use scheng_param_store::NodeConfigBuilder;
use scheng_control_osc_wgpu::OscReceiver;
use scheng_hotreload::HotReloader;
use tauri::{AppHandle, Emitter};

use crate::{
    engine::{AppState, OutputMode},
    preview,
};

/// Emit a preview every N render frames.
/// At 30fps, 2 = 15fps preview. At 60fps, 4 = 15fps preview.
const PREVIEW_INTERVAL: u64 = 2;

/// Target render framerate. The render loop sleeps to hit this.
const TARGET_FPS: f64 = 30.0;
const FRAME_BUDGET: Duration = Duration::from_nanos((1_000_000_000.0 / TARGET_FPS) as u64);

/// Entry point for the render thread.
pub fn run(app: AppHandle, state: AppState) {
    log::info!("Render thread started");

    // ── GPU init ──────────────────────────────────────────────────────────
    let (width, height) = {
        let cfg = state.render_config.lock().unwrap();
        (cfg.width, cfg.height)
    };

    let mut runtime = match WgpuRuntime::new(width, height) {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to init WgpuRuntime: {e}");
            return;
        }
    };

    // Write adapter info back to shared state
    {
        *state.adapter_name.lock().unwrap() = runtime.ctx.adapter_info.name.clone();
        *state.gpu_ready.lock().unwrap() = true;
    }
    log::info!("GPU ready: {}", runtime.ctx.adapter_info.name);

    // ── Default graph: ShaderSource → PixelsOut ───────────────────────────
    let (graph, plan, src_node, out_node) = build_default_graph();

    // ── NodeConfig builder ────────────────────────────────────────────────
    let mut builder = NodeConfigBuilder::new();
    builder.register("src", src_node);
    builder.register("out", out_node);

    // Load initial shaders if present, else use built-in animated gradient
    if let Ok(frag) = std::fs::read_to_string("assets/shaders/default.frag") {
        builder.set_shader(src_node, frag);
    }

    // ── OSC receiver ─────────────────────────────────────────────────────
    let mut osc = OscReceiver::bind("127.0.0.1:9000")
        .map_err(|e| log::warn!("OSC not available: {e}"))
        .ok();

    // ── Hot-reload watcher ────────────────────────────────────────────────
    let mut reloader = HotReloader::new("assets/")
        .map_err(|e| log::warn!("Hot-reload not available: {e}"))
        .ok();
    if let Some(ref mut r) = reloader {
        r.register_shader("assets/shaders/default.frag", src_node);
    }

    // ── Output sink ───────────────────────────────────────────────────────
    // Phase 5: use PixelReadbackSink as the baseline — OutputMode switching
    // wires in FfmpegSink / SyphonSink based on render_config.output_mode.
    let mut readback_sink = PixelReadbackSink::new();
    let mut ffmpeg_sink: Option<scheng_output_ffmpeg::FfmpegSink> = None;

    // ── Render loop ───────────────────────────────────────────────────────
    let start  = Instant::now();
    let mut frame: u64 = 0;

    loop {
        let frame_start = Instant::now();

        // 1. OSC
        if let Some(ref mut osc_recv) = osc {
            let mut store = state.param_store.lock().unwrap();
            osc_recv.poll(&mut store);
        }

        // 2. Hot-reload
        if let Some(ref mut r) = reloader {
            let mut store = state.param_store.lock().unwrap();
            let reloaded = r.check(&mut builder, &mut store);
            if reloaded > 0 {
                let _ = app.emit("params-reloaded", ());
            }
        }

        // 3. Step smoothed parameter values
        state.param_store.lock().unwrap().step_frame();

        // 4. Build NodeConfigs
        let configs = {
            let store = state.param_store.lock().unwrap();
            builder.build(&store)
        };

        // 5. Read render config snapshot (cheap — one lock, immediate release)
        let (out_mode, is_recording, rec_path, w, h) = {
            let cfg = state.render_config.lock().unwrap();
            (cfg.output_mode.clone(), cfg.is_recording,
             cfg.record_path.clone(), cfg.width, cfg.height)
        };

        let ctx = FrameCtx {
            width: w, height: h,
            time:  start.elapsed().as_secs_f32(),
            frame,
        };

        // 6. GPU: execute frame
        // Route to the correct OutputSink based on mode
        let render_result = match out_mode {
            OutputMode::Preview => {
                // Readback only when preview IPC is needed
                if frame % PREVIEW_INTERVAL == 0 {
                    runtime.execute_frame(&graph, &plan, &configs, &ctx, &mut readback_sink)
                } else {
                    // Dummy sink — just renders, no readback
                    let mut noop = NoopSink;
                    runtime.execute_frame(&graph, &plan, &configs, &ctx, &mut noop)
                }
            }
            OutputMode::Record => {
                // Start FFmpeg if not already running
                if ffmpeg_sink.is_none() {
                    ffmpeg_sink = start_ffmpeg_recording(&rec_path, w, h);
                }
                if let Some(ref mut sink) = ffmpeg_sink {
                    runtime.execute_frame(&graph, &plan, &configs, &ctx, sink)
                } else {
                    let mut noop = NoopSink;
                    runtime.execute_frame(&graph, &plan, &configs, &ctx, &mut noop)
                }
            }
            OutputMode::Stream => {
                // Same as Record but with RTSP URL
                if ffmpeg_sink.is_none() {
                    let url = state.render_config.lock().unwrap().stream_url.clone();
                    ffmpeg_sink = start_ffmpeg_stream(&url, w, h);
                }
                if let Some(ref mut sink) = ffmpeg_sink {
                    runtime.execute_frame(&graph, &plan, &configs, &ctx, sink)
                } else {
                    let mut noop = NoopSink;
                    runtime.execute_frame(&graph, &plan, &configs, &ctx, &mut noop)
                }
            }
            // Syphon / Spout / NDI: Phase 5 stub — preview until wired
            _ => {
                let mut noop = NoopSink;
                runtime.execute_frame(&graph, &plan, &configs, &ctx, &mut noop)
            }
        };

        if let Err(e) = render_result {
            log::error!("execute_frame error: {e}");
        }

        // Stop FFmpeg if recording was disabled
        if !is_recording {
            if let Some(mut sink) = ffmpeg_sink.take() {
                sink.stop();
            }
        }

        // 7. Preview: emit JPEG to WebView at ~15fps
        if frame % PREVIEW_INTERVAL == 0 {
            if let Some(pixels) = readback_sink.take_pixels(out_node) {
                preview::emit_preview(&app, &pixels, w, h);
            }
        }

        // 8. Update frame counter
        *state.frame_count.lock().unwrap() = frame;
        frame += 1;

        // 9. Sleep to hit target framerate
        let elapsed = frame_start.elapsed();
        if elapsed < FRAME_BUDGET {
            std::thread::sleep(FRAME_BUDGET - elapsed);
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn build_default_graph() -> (Graph, scheng_graph::Plan, scheng_graph::NodeId, scheng_graph::NodeId) {
    let mut g   = Graph::new();
    let src     = g.add_node(NodeKind::ShaderSource);
    let out     = g.add_node(NodeKind::PixelsOut);
    g.connect_named(src, "out", out, "in").expect("default graph wiring failed");
    let plan    = g.compile().expect("default graph compile failed");
    (g, plan, src, out)
}

fn start_ffmpeg_recording(path: &str, w: u32, h: u32) -> Option<scheng_output_ffmpeg::FfmpegSink> {
    use scheng_output_ffmpeg::{FfmpegConfig, FfmpegSink};
    use scheng_output_ffmpeg::config::OutputTarget;
    let config = FfmpegConfig {
        width: w, height: h, framerate: TARGET_FPS as u32,
        target: OutputTarget::File { path: path.into(), overwrite: true },
        ..Default::default()
    };
    FfmpegSink::new(config).map_err(|e| log::error!("FFmpeg recording start failed: {e}")).ok()
}

fn start_ffmpeg_stream(url: &str, w: u32, h: u32) -> Option<scheng_output_ffmpeg::FfmpegSink> {
    use scheng_output_ffmpeg::{FfmpegConfig, FfmpegSink};
    use scheng_output_ffmpeg::config::OutputTarget;
    let config = FfmpegConfig {
        width: w, height: h, framerate: TARGET_FPS as u32,
        target: OutputTarget::Rtsp { url: url.into() },
        ..Default::default()
    };
    FfmpegSink::new(config).map_err(|e| log::error!("FFmpeg stream start failed: {e}")).ok()
}

// ── NoopSink — renders without readback ──────────────────────────────────

struct NoopSink;

impl scheng_runtime_wgpu::executor::OutputSink for NoopSink {
    fn present(&mut self, _: scheng_graph::NodeId, _: &scheng_runtime_wgpu::RenderTarget,
               _: &FrameCtx, _: &wgpu::Device, _: &wgpu::Queue) {}
}
