//! `tests/headless.rs` — integration tests for scheng-runtime-wgpu.
//!
//! These tests run the full pipeline (context → shader → render → readback)
//! on actual GPU hardware. They are skipped gracefully if no GPU adapter
//! is available (common in CI without a GPU).
//!
//! # Running
//!
//! ```sh
//! # Run all headless tests with output visible
//! cargo test -p scheng-runtime-wgpu -- --nocapture
//!
//! # Run a single test
//! cargo test -p scheng-runtime-wgpu test_single_node_renders -- --nocapture
//!
//! # Force software rendering (WGPU_POWER_PREF=none and Vulkan software on Linux)
//! WGPU_BACKEND=gl cargo test -p scheng-runtime-wgpu -- --nocapture
//! ```
//!
//! # What these tests verify
//!
//! 1. `test_compat_preprocessing` — pure CPU: compat.rs strips/rewrites correctly
//! 2. `test_naga_glsl_compile`     — GPU-free: naga parses a processed shader
//! 3. `test_context_init`          — GPU: WgpuContext::new() succeeds
//! 4. `test_single_node_renders`   — GPU: ShaderSource → PixelsOut, pixel readback
//! 5. `test_two_node_pipeline`     — GPU: ShaderSource → ShaderPass → PixelsOut
//! 6. `test_uniform_time_varies`   — GPU: uTime changes between frames
//! 7. `test_blank_texture_safety`  — GPU: unconnected iChannelN → black, not crash

use std::collections::HashMap;

use scheng_graph::{Graph, NodeKind};
use scheng_runtime_wgpu::{
    executor::{NodeConfig, OutputSink, PixelReadbackSink},
    WgpuRuntime,
    FrameCtx,
};

// ── Helper: try_init ──────────────────────────────────────────────────────

/// Try to create a WgpuRuntime for testing.
/// Returns None if no GPU adapter is available — the test should skip.
fn try_runtime(width: u32, height: u32) -> Option<WgpuRuntime> {
    match WgpuRuntime::new(width, height) {
        Ok(r) => Some(r),
        Err(scheng_runtime_wgpu::WgpuError::NoAdapter) => {
            eprintln!("[skip] No GPU adapter available — skipping GPU tests.");
            eprintln!("       Set WGPU_BACKEND=gl to try software rendering.");
            None
        }
        Err(e) => {
            panic!("Unexpected WgpuRuntime init error: {:?}", e);
        }
    }
}

/// Shorthand: assert a Vec<u8> of RGBA pixels is not all-zero (i.e., something rendered).
fn assert_has_non_zero_pixels(pixels: &[u8], label: &str) {
    let any_nonzero = pixels.iter().any(|&b| b != 0);
    assert!(
        any_nonzero,
        "{}: all pixels are zero — shader did not produce visible output.\n\
         First 16 bytes: {:?}",
        label,
        &pixels[..pixels.len().min(16)]
    );
}

// ── Test 1: compat preprocessing (CPU only, no GPU needed) ───────────────

#[test]
fn test_compat_preprocessing() {
    use scheng_runtime_wgpu::compat; // expose via lib.rs re-export or test directly

    // NOTE: if compat is not pub in lib.rs, adjust this to import from the module directly.
    // For now, test through the public API by checking processed output.

    let shader = r#"
#version 330 core
in vec2 v_uv;
out vec4 fragColor;
uniform float uTime;
uniform vec2 uResolution;
uniform sampler2D iChannel0;

void main() {
    fragColor = texture(iChannel0, v_uv) * vec4(uTime);
}
"#;

    let processed = compat::process(shader, "test_node");

    // Should have version 450
    assert!(processed.source.contains("#version 450"), "Version not updated");

    // Should have the compat header's layout qualifiers
    assert!(processed.source.contains("layout(binding = 0) uniform texture2D iChannel0_tex"),
        "iChannel0_tex binding missing");
    assert!(processed.source.contains("layout(binding = 5) uniform FrameBlock"),
        "FrameBlock binding missing");

    // iChannel0 usage should be rewritten
    assert!(processed.source.contains("sampler2D(iChannel0_tex, iSampler)"),
        "iChannel0 not rewritten to split form");

    // void main() must survive
    assert!(processed.source.contains("void main()"), "main() was lost");

    // No custom uniforms in this shader
    assert!(processed.custom_uniform_names.is_empty(), "Unexpected custom uniforms");

    eprintln!("[PASS] test_compat_preprocessing");
}

// ── Test 2: naga GLSL compilation (CPU only) ─────────────────────────────

#[test]
fn test_naga_glsl_compile() {
    use scheng_runtime_wgpu::compat;
    use naga::front::glsl as naga_glsl;
    use naga::valid::{Capabilities, ValidationFlags, Validator};

    let shader = r#"
void main() {
    fragColor = vec4(v_uv, 0.5, 1.0);
}
"#;

    let processed = compat::process(shader, "naga_test");
    eprintln!("Processed source:\n{}", processed.source);

    let mut frontend = naga_glsl::Frontend::default();
    let options = naga_glsl::Options {
        stage: naga::ShaderStage::Fragment,
        defines: Default::default(),
    };
    let module = frontend.parse(&options, &processed.source)
        .expect("naga GLSL parse failed");

    let mut validator = Validator::new(ValidationFlags::all(), Capabilities::empty());
    validator.validate(&module).expect("naga validation failed");

    eprintln!("[PASS] test_naga_glsl_compile — naga parsed and validated the processed shader");
}

// ── Test 3: GPU context init ──────────────────────────────────────────────

#[test]
fn test_context_init() {
    let runtime = match try_runtime(16, 16) {
        Some(r) => r,
        None => return,
    };
    eprintln!(
        "[PASS] test_context_init — adapter: {}",
        runtime.ctx.adapter_info.name
    );
}

// ── Test 4: single node renders non-zero pixels ───────────────────────────

#[test]
fn test_single_node_renders() {
    let mut runtime = match try_runtime(64, 64) {
        Some(r) => r,
        None => return,
    };

    // Build a minimal graph: ShaderSource → PixelsOut
    let mut g = Graph::new();
    let src = g.add_node(NodeKind::ShaderSource);
    let out = g.add_node(NodeKind::PixelsOut);
    g.connect_named(src, "out", out, "in").expect("connect failed");
    let plan = g.compile().expect("compile failed");

    // Default configs (uses built-in animated gradient shader)
    let mut configs = HashMap::new();
    configs.insert(src, NodeConfig::default());
    configs.insert(out, NodeConfig::default());

    let ctx = FrameCtx { width: 64, height: 64, time: 1.0, frame: 1 };
    let mut sink = PixelReadbackSink::new();

    runtime.execute_frame(&plan, &configs, &ctx, &mut sink)
        .expect("execute_frame failed");

    let pixels = sink.take_pixels(out).expect("No pixels from output node");
    assert_eq!(
        pixels.len(), 64 * 64 * 4,
        "Expected 64×64×4 bytes, got {}", pixels.len()
    );
    assert_has_non_zero_pixels(&pixels, "test_single_node_renders");

    eprintln!("[PASS] test_single_node_renders — {} bytes, first pixel: {:?}",
        pixels.len(), &pixels[..4]);
}

// ── Test 5: two-node pipeline ─────────────────────────────────────────────

#[test]
fn test_two_node_pipeline() {
    let mut runtime = match try_runtime(32, 32) {
        Some(r) => r,
        None => return,
    };

    // ShaderSource → ShaderPass → PixelsOut
    let mut g = Graph::new();
    let src  = g.add_node(NodeKind::ShaderSource);
    let pass = g.add_node(NodeKind::ShaderPass);
    let out  = g.add_node(NodeKind::PixelsOut);
    g.connect_named(src,  "out", pass, "in").unwrap();
    g.connect_named(pass, "out", out,  "in").unwrap();
    let plan = g.compile().unwrap();

    let mut configs = HashMap::new();
    configs.insert(src,  NodeConfig::default());
    configs.insert(pass, NodeConfig::default());
    configs.insert(out,  NodeConfig::default());

    let ctx = FrameCtx { width: 32, height: 32, time: 0.5, frame: 5 };
    let mut sink = PixelReadbackSink::new();

    runtime.execute_frame(&plan, &configs, &ctx, &mut sink).unwrap();

    let pixels = sink.take_pixels(out).expect("No pixels");
    assert_eq!(pixels.len(), 32 * 32 * 4);
    assert_has_non_zero_pixels(&pixels, "test_two_node_pipeline");

    eprintln!("[PASS] test_two_node_pipeline");
}

// ── Test 6: uTime propagates (different frames → different pixels) ─────────

#[test]
fn test_uniform_time_varies() {
    let mut runtime = match try_runtime(32, 32) {
        Some(r) => r,
        None => return,
    };

    let mut g = Graph::new();
    let src = g.add_node(NodeKind::ShaderSource);
    let out = g.add_node(NodeKind::PixelsOut);
    g.connect_named(src, "out", out, "in").unwrap();
    let plan = g.compile().unwrap();

    let mut configs = HashMap::new();
    configs.insert(src, NodeConfig::default());
    configs.insert(out, NodeConfig::default());

    // Frame 0: time = 0.0
    let ctx0 = FrameCtx { width: 32, height: 32, time: 0.0, frame: 0 };
    let mut sink0 = PixelReadbackSink::new();
    runtime.execute_frame(&plan, &configs, &ctx0, &mut sink0).unwrap();
    let pixels0 = sink0.take_pixels(out).unwrap();

    // Frame 1: time = 1.5 (the built-in shader animates with uTime)
    let ctx1 = FrameCtx { width: 32, height: 32, time: 1.5, frame: 1 };
    let mut sink1 = PixelReadbackSink::new();
    runtime.execute_frame(&plan, &configs, &ctx1, &mut sink1).unwrap();
    let pixels1 = sink1.take_pixels(out).unwrap();

    // The two frames should differ (built-in uses sin(uTime) which changes)
    assert_ne!(pixels0, pixels1,
        "Frames at t=0 and t=1.5 are identical — uTime is not varying");

    eprintln!("[PASS] test_uniform_time_varies — frames differ as expected");
}

// ── Test 7: unconnected iChannelN → black, not a crash ───────────────────

#[test]
fn test_blank_texture_safety() {
    let mut runtime = match try_runtime(16, 16) {
        Some(r) => r,
        None => return,
    };

    // ShaderPass with no input (iChannel0 will be the blank 1×1 texture)
    // Use a passthrough shader — it will sample the blank texture.
    let mut g = Graph::new();
    let pass = g.add_node(NodeKind::ShaderPass);
    let out  = g.add_node(NodeKind::PixelsOut);
    g.connect_named(pass, "out", out, "in").unwrap();
    // NOTE: no node is connected to pass's "in" port.
    let plan = g.compile().unwrap();

    let mut configs = HashMap::new();
    configs.insert(pass, NodeConfig {
        frag_shader: Some(r#"
            void main() {
                // Sample iChannel0 (blank) and add a small constant to verify it renders
                vec4 c = texture(iChannel0, v_uv);
                fragColor = c + vec4(0.1, 0.0, 0.0, 1.0); // always produces R=0.1
            }
        "#.to_owned()),
        ..Default::default()
    });
    configs.insert(out, NodeConfig::default());

    let ctx = FrameCtx { width: 16, height: 16, time: 0.0, frame: 0 };
    let mut sink = PixelReadbackSink::new();

    // Must not panic even with an unconnected input
    runtime.execute_frame(&plan, &configs, &ctx, &mut sink)
        .expect("blank texture test crashed");

    let pixels = sink.take_pixels(out).expect("no pixels");
    assert_has_non_zero_pixels(&pixels, "test_blank_texture_safety (R=0.1 should be visible)");

    eprintln!("[PASS] test_blank_texture_safety");
}

// ── Test 8: custom user shader ─────────────────────────────────────────────

#[test]
fn test_custom_frag_shader() {
    let mut runtime = match try_runtime(32, 32) {
        Some(r) => r,
        None => return,
    };

    let mut g = Graph::new();
    let src = g.add_node(NodeKind::ShaderSource);
    let out = g.add_node(NodeKind::PixelsOut);
    g.connect_named(src, "out", out, "in").unwrap();
    let plan = g.compile().unwrap();

    // Custom shader: solid red — trivially verifiable
    let mut configs = HashMap::new();
    configs.insert(src, NodeConfig {
        frag_shader: Some(r#"
#version 330 core
in vec2 v_uv;
out vec4 fragColor;

void main() {
    fragColor = vec4(1.0, 0.0, 0.0, 1.0); // solid red
}
"#.to_owned()),
        ..Default::default()
    });
    configs.insert(out, NodeConfig::default());

    let ctx = FrameCtx { width: 32, height: 32, time: 0.0, frame: 0 };
    let mut sink = PixelReadbackSink::new();
    runtime.execute_frame(&plan, &configs, &ctx, &mut sink).unwrap();

    let pixels = sink.take_pixels(out).unwrap();
    assert_eq!(pixels.len(), 32 * 32 * 4);

    // Every pixel should be R=255, G=0, B=0, A=255
    for chunk in pixels.chunks(4) {
        assert_eq!(chunk[0], 255, "Red channel should be 255, got {}", chunk[0]);
        assert_eq!(chunk[1], 0,   "Green channel should be 0");
        assert_eq!(chunk[2], 0,   "Blue channel should be 0");
        assert_eq!(chunk[3], 255, "Alpha should be 255");
    }

    eprintln!("[PASS] test_custom_frag_shader — solid red verified pixel-perfect");
}
