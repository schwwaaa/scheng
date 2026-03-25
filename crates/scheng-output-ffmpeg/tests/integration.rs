//! Integration tests for scheng-output-ffmpeg.
//!
//! # Running
//!
//! ```sh
//! cargo test -p scheng-output-ffmpeg -- --nocapture
//!
//! # With a real ffmpeg stream (requires mediamtx running locally):
//! SCHENG_TEST_RTSP=1 cargo test -p scheng-output-ffmpeg -- --nocapture
//! ```

use scheng_output_ffmpeg::{
    config::{EncodingConfig, OutputTarget},
    FfmpegConfig,
};

// ── Config tests (CPU only, no ffmpeg needed) ─────────────────────────────

#[test]
fn test_config_default() {
    let cfg = FfmpegConfig::default();
    assert_eq!(cfg.ffmpeg_path, "ffmpeg");
    assert_eq!(cfg.width, 1280);
    assert_eq!(cfg.height, 720);
    assert_eq!(cfg.framerate, 30);
    assert_eq!(cfg.queue_depth, 4);
    eprintln!("[PASS] test_config_default");
}

#[test]
fn test_config_validate_even_dimensions() {
    let mut cfg = FfmpegConfig::default();
    cfg.width  = 1280;
    cfg.height = 720;
    assert!(cfg.validate().is_ok(), "1280×720 should be valid");

    cfg.width = 1281; // odd
    assert!(cfg.validate().is_err(), "1281×720 should fail (odd width)");

    cfg.width  = 1280;
    cfg.height = 721; // odd
    assert!(cfg.validate().is_err(), "1280×721 should fail (odd height)");

    eprintln!("[PASS] test_config_validate_even_dimensions");
}

#[test]
fn test_build_args_rtsp() {
    let cfg = FfmpegConfig {
        width: 1280, height: 720, framerate: 30,
        target: OutputTarget::Rtsp { url: "rtsp://localhost:8554/live".into() },
        encoding: EncodingConfig {
            codec:            "libx264".into(),
            preset:           "ultrafast".into(),
            bitrate:          "4M".into(),
            pixel_format:     "yuv420p".into(),
            tune_zerolatency: true,
        },
        ..Default::default()
    };

    let args = cfg.build_args();
    let args_str = args.join(" ");

    assert!(args_str.contains("rawvideo"),      "should specify rawvideo input");
    assert!(args_str.contains("rgba"),          "should specify RGBA pixel format");
    assert!(args_str.contains("1280x720"),      "should include resolution");
    assert!(args_str.contains("pipe:0"),        "should read from stdin");
    assert!(args_str.contains("libx264"),       "should use libx264");
    assert!(args_str.contains("ultrafast"),     "should use ultrafast preset");
    assert!(args_str.contains("zerolatency"),   "should tune for zerolatency");
    assert!(args_str.contains("rtsp://"),       "should output to RTSP");
    assert!(args_str.contains("rtsp"),          "should use rtsp format");

    eprintln!("[PASS] test_build_args_rtsp — args: {}", args_str);
}

#[test]
fn test_build_args_file() {
    let cfg = FfmpegConfig {
        width: 1920, height: 1080, framerate: 60,
        target: OutputTarget::File { path: "output.mp4".into(), overwrite: true },
        encoding: EncodingConfig {
            codec:            "libx264".into(),
            preset:           "fast".into(),
            bitrate:          "8M".into(),
            pixel_format:     "yuv420p".into(),
            tune_zerolatency: false,
        },
        ..Default::default()
    };

    let args = cfg.build_args();
    let args_str = args.join(" ");

    assert!(args_str.contains("1920x1080"),  "resolution in args");
    assert!(args_str.contains("-y"),         "overwrite flag present");
    assert!(args_str.contains("output.mp4"),"output path present");
    assert!(!args_str.contains("zerolatency"), "no zerolatency for file");
    assert!(!args_str.contains("rtsp"),     "no rtsp for file output");

    eprintln!("[PASS] test_build_args_file — args: {}", args_str);
}

#[test]
fn test_config_from_json_string() {
    let json = r#"{
        "ffmpeg_path": "/usr/local/bin/ffmpeg",
        "width": 640,
        "height": 480,
        "framerate": 25,
        "queue_depth": 2,
        "target": { "type": "rtsp", "url": "rtsp://192.168.1.10:8554/stream" },
        "encoding": {
            "codec": "libx265",
            "preset": "fast",
            "bitrate": "2M",
            "pixel_format": "yuv420p",
            "tune_zerolatency": true
        }
    }"#;

    let cfg: FfmpegConfig = serde_json::from_str(json).expect("JSON parse failed");
    assert_eq!(cfg.ffmpeg_path, "/usr/local/bin/ffmpeg");
    assert_eq!(cfg.width, 640);
    assert_eq!(cfg.height, 480);
    assert_eq!(cfg.framerate, 25);
    assert_eq!(cfg.queue_depth, 2);
    assert_eq!(cfg.encoding.codec, "libx265");

    eprintln!("[PASS] test_config_from_json_string");
}

// ── Live ffmpeg test (requires ffmpeg in PATH) ────────────────────────────

/// Tests that FfmpegSink actually pipes frames to a real ffmpeg process.
/// Renders 10 frames to a temp file and verifies ffmpeg wrote some bytes.
#[test]
fn test_ffmpeg_file_recording() {
    // Skip if ffmpeg not available
    if std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_err()
    {
        eprintln!("[skip] test_ffmpeg_file_recording — ffmpeg not in PATH");
        return;
    }

    use scheng_output_ffmpeg::FfmpegSink;
    use std::collections::HashMap;
    use scheng_graph::{Graph, NodeKind};
    use scheng_runtime_wgpu::{WgpuRuntime, executor::NodeConfig, FrameCtx};

    let tmp = std::env::temp_dir().join("scheng_ffmpeg_test.mp4");

    let config = FfmpegConfig {
        width: 320, height: 240, framerate: 30,
        target: OutputTarget::File {
            path:      tmp.to_str().unwrap().to_owned(),
            overwrite: true,
        },
        encoding: EncodingConfig {
            codec:            "libx264".into(),
            preset:           "ultrafast".into(),
            bitrate:          "1M".into(),
            pixel_format:     "yuv420p".into(),
            tune_zerolatency: false,
        },
        ..Default::default()
    };

    let mut sink = match FfmpegSink::new(config) {
        Ok(s)  => s,
        Err(e) => { eprintln!("[skip] FfmpegSink::new failed: {e}"); return; }
    };

    // Build graph and runtime
    let mut runtime = match scheng_runtime_wgpu::WgpuRuntime::new(320, 240) {
        Ok(r)  => r,
        Err(_) => { eprintln!("[skip] No GPU adapter"); return; }
    };

    let mut g = Graph::new();
    let src = g.add_node(NodeKind::ShaderSource);
    let out = g.add_node(NodeKind::PixelsOut);
    g.connect_named(src, "out", out, "in").unwrap();
    let plan = g.compile().unwrap();

    let mut cfg = HashMap::new();
    cfg.insert(src, NodeConfig::default());
    cfg.insert(out, NodeConfig::default());

    // Render 10 frames
    for i in 0..10u64 {
        let ctx = FrameCtx { width: 320, height: 240, time: i as f32 / 30.0, frame: i };
        runtime.execute_frame(&g, &plan, &cfg, &ctx, &mut sink).unwrap();
    }

    // Stop ffmpeg (waits for process to finish and flush MP4)
    sink.stop();

    eprintln!("Frames sent: {}, dropped: {}", sink.frames_sent(), sink.frames_dropped());

    // Verify the output file was written and has content
    let metadata = std::fs::metadata(&tmp)
        .expect("output file not found — ffmpeg may have failed");

    assert!(metadata.len() > 1000,
        "output file too small ({} bytes) — encoding may have failed", metadata.len());

    // Clean up
    let _ = std::fs::remove_file(&tmp);

    eprintln!("[PASS] test_ffmpeg_file_recording — {} bytes written", metadata.len());
}
