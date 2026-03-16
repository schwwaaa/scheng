//! Integration tests — run with: cargo test -p scheng-runtime-wgpu -- --nocapture

use std::collections::HashMap;
use scheng_graph::{Graph, NodeKind};
use scheng_runtime_wgpu::{executor::{NodeConfig, PixelReadbackSink}, FrameCtx, WgpuRuntime};

fn try_runtime(w: u32, h: u32) -> Option<WgpuRuntime> {
    match WgpuRuntime::new(w, h) {
        Ok(r)  => Some(r),
        Err(scheng_runtime_wgpu::WgpuError::NoAdapter) => {
            eprintln!("[skip] No GPU adapter. Try: WGPU_BACKEND=gl cargo test");
            None
        }
        Err(e) => panic!("init error: {e:?}"),
    }
}

fn assert_nonzero(px: &[u8], label: &str) {
    assert!(px.iter().any(|&b| b != 0), "{label}: all pixels are zero");
}

#[test]
fn test_naga_compile_cpu_only() {
    use naga::front::glsl as g;
    use naga::valid::{Capabilities, ValidationFlags, Validator};
    let src = r#"#version 450
layout(location=0) in vec2 v_uv;
layout(location=0) out vec4 fragColor;
layout(binding=5) uniform FrameBlock { vec2 uResolution; float uTime; uint uFrame; };
void main() { fragColor = vec4(v_uv, 0.5*sin(uTime)+0.5, 1.0); }
"#;
    let mut fe = g::Frontend::default();
    let opts   = g::Options { stage: naga::ShaderStage::Fragment, defines: Default::default() };
    let m      = fe.parse(&opts, src).expect("naga parse failed");
    Validator::new(ValidationFlags::all(), Capabilities::empty()).validate(&m).expect("validation failed");
    eprintln!("[PASS] test_naga_compile_cpu_only");
}

#[test]
fn test_context_init() {
    let r = match try_runtime(16, 16) { Some(r) => r, None => return };
    eprintln!("[PASS] test_context_init — {}", r.ctx.adapter_info.name);
}

#[test]
fn test_single_node_renders() {
    let mut r = match try_runtime(64, 64) { Some(r) => r, None => return };
    let mut g = Graph::new();
    let src = g.add_node(NodeKind::ShaderSource);
    let out = g.add_node(NodeKind::PixelsOut);
    g.connect_named(src, "out", out, "in").unwrap();
    let plan = g.compile().unwrap();
    let mut cfg = HashMap::new();
    cfg.insert(src, NodeConfig::default());
    cfg.insert(out, NodeConfig::default());
    let ctx = FrameCtx { width: 64, height: 64, time: 1.0, frame: 1 };
    let mut sink = PixelReadbackSink::new();
    r.execute_frame(&g, &plan, &cfg, &ctx, &mut sink).unwrap();
    let px = sink.take_pixels(out).unwrap();
    assert_eq!(px.len(), 64 * 64 * 4);
    assert_nonzero(&px, "test_single_node_renders");
    eprintln!("[PASS] test_single_node_renders — first px {:?}", &px[..4]);
}

#[test]
fn test_two_node_pipeline() {
    let mut r = match try_runtime(32, 32) { Some(r) => r, None => return };
    let mut g = Graph::new();
    let src  = g.add_node(NodeKind::ShaderSource);
    let pass = g.add_node(NodeKind::ShaderPass);
    let out  = g.add_node(NodeKind::PixelsOut);
    g.connect_named(src,  "out", pass, "in").unwrap();
    g.connect_named(pass, "out", out,  "in").unwrap();
    let plan = g.compile().unwrap();
    let mut cfg = HashMap::new();
    cfg.insert(src,  NodeConfig::default());
    cfg.insert(pass, NodeConfig::default());
    cfg.insert(out,  NodeConfig::default());
    let ctx = FrameCtx { width: 32, height: 32, time: 0.5, frame: 5 };
    let mut sink = PixelReadbackSink::new();
    r.execute_frame(&g, &plan, &cfg, &ctx, &mut sink).unwrap();
    let px = sink.take_pixels(out).unwrap();
    assert_eq!(px.len(), 32 * 32 * 4);
    assert_nonzero(&px, "test_two_node_pipeline");
    eprintln!("[PASS] test_two_node_pipeline");
}

#[test]
fn test_time_varies_between_frames() {
    let mut r = match try_runtime(32, 32) { Some(r) => r, None => return };
    let mut g = Graph::new();
    let src = g.add_node(NodeKind::ShaderSource);
    let out = g.add_node(NodeKind::PixelsOut);
    g.connect_named(src, "out", out, "in").unwrap();
    let plan = g.compile().unwrap();
    let mut cfg = HashMap::new();
    cfg.insert(src, NodeConfig::default());
    cfg.insert(out, NodeConfig::default());
    let mut s0 = PixelReadbackSink::new();
    r.execute_frame(&g, &plan, &cfg, &FrameCtx { width:32, height:32, time:0.0, frame:0 }, &mut s0).unwrap();
    let p0 = s0.take_pixels(out).unwrap();
    let mut s1 = PixelReadbackSink::new();
    r.execute_frame(&g, &plan, &cfg, &FrameCtx { width:32, height:32, time:1.5, frame:1 }, &mut s1).unwrap();
    let p1 = s1.take_pixels(out).unwrap();
    assert_ne!(p0, p1, "uTime not affecting output");
    eprintln!("[PASS] test_time_varies_between_frames");
}

#[test]
fn test_custom_shader_solid_red() {
    let mut r = match try_runtime(16, 16) { Some(r) => r, None => return };
    let mut g = Graph::new();
    let src = g.add_node(NodeKind::ShaderSource);
    let out = g.add_node(NodeKind::PixelsOut);
    g.connect_named(src, "out", out, "in").unwrap();
    let plan = g.compile().unwrap();
    let mut cfg = HashMap::new();
    cfg.insert(src, NodeConfig {
        frag_shader: Some("#version 330 core\nin vec2 v_uv;\nout vec4 fragColor;\nvoid main(){fragColor=vec4(1.0,0.0,0.0,1.0);}".into()),
        output_name: None,
    });
    cfg.insert(out, NodeConfig::default());
    let ctx = FrameCtx { width:16, height:16, time:0.0, frame:0 };
    let mut sink = PixelReadbackSink::new();
    r.execute_frame(&g, &plan, &cfg, &ctx, &mut sink).unwrap();
    let px = sink.take_pixels(out).unwrap();
    for chunk in px.chunks(4) {
        assert_eq!(chunk[0], 255, "R=255");
        assert_eq!(chunk[1], 0,   "G=0");
        assert_eq!(chunk[2], 0,   "B=0");
        assert_eq!(chunk[3], 255, "A=255");
    }
    eprintln!("[PASS] test_custom_shader_solid_red");
}
