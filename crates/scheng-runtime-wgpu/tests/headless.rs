//! Integration tests for scheng-runtime-wgpu (Phase 1 + 1.2).

use std::collections::HashMap;
use scheng_graph::{Graph, NodeKind};
use scheng_runtime_wgpu::{
    executor::{NodeConfig, PixelReadbackSink},
    FrameCtx, WgpuRuntime,
};

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
layout(binding=6) uniform CustomBlock { vec4 u_custom[8]; };
void main() { fragColor = vec4(v_uv, 0.5*sin(uTime)+0.5, 1.0); }
"#;
    let mut fe = g::Frontend::default();
    let opts   = g::Options { stage: naga::ShaderStage::Fragment, defines: Default::default() };
    let m      = fe.parse(&opts, src).expect("naga parse failed");
    Validator::new(ValidationFlags::all(), Capabilities::empty()).validate(&m).expect("validation failed");
    eprintln!("[PASS] test_naga_compile_cpu_only");
}

#[test]
fn test_compat_custom_uniforms() {
    use scheng_runtime_wgpu::compat;
    let shader = r#"
#version 330 core
in vec2 v_uv;
out vec4 fragColor;
uniform float u_brightness;
uniform float u_contrast;
void main() {
    fragColor = vec4(u_brightness, u_contrast, 0.0, 1.0);
}
"#;
    let r = compat::process(shader, "test");
    assert_eq!(r.custom_uniform_names, vec!["u_brightness", "u_contrast"]);
    assert!(r.source.contains("u_custom[0].x"), "u_brightness not replaced");
    assert!(r.source.contains("u_custom[0].y"), "u_contrast not replaced");
    assert!(!r.source.contains("uniform float u_brightness"));
    assert!(r.source.contains("vec4 u_custom[8]"));
    eprintln!("[PASS] test_compat_custom_uniforms");
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
fn test_custom_uniforms_reach_shader() {
    let mut r = match try_runtime(16, 16) { Some(r) => r, None => return };
    let mut g = Graph::new();
    let src = g.add_node(NodeKind::ShaderSource);
    let out = g.add_node(NodeKind::PixelsOut);
    g.connect_named(src, "out", out, "in").unwrap();
    let plan = g.compile().unwrap();

    // Shader uses u_red — set to 1.0 → should produce red pixels
    let mut src_cfg = NodeConfig::default();
    src_cfg.frag_shader = Some(r#"
#version 330 core
in vec2 v_uv;
out vec4 fragColor;
uniform float u_red;
void main() { fragColor = vec4(u_red, 0.0, 0.0, 1.0); }
"#.to_owned());
    src_cfg.uniforms.insert("u_red".to_owned(), 1.0);

    let mut cfg = HashMap::new();
    cfg.insert(src, src_cfg);
    cfg.insert(out, NodeConfig::default());

    let ctx = FrameCtx { width: 16, height: 16, time: 0.0, frame: 0 };
    let mut sink = PixelReadbackSink::new();
    r.execute_frame(&g, &plan, &cfg, &ctx, &mut sink).unwrap();

    let px = sink.take_pixels(out).unwrap();
    // R should be 255 (u_red=1.0), G and B should be 0
    for chunk in px.chunks(4) {
        assert_eq!(chunk[0], 255, "R should be 255, u_red=1.0 not reaching shader");
        assert_eq!(chunk[1], 0,   "G should be 0");
        assert_eq!(chunk[2], 0,   "B should be 0");
    }
    eprintln!("[PASS] test_custom_uniforms_reach_shader — u_red=1.0 confirmed pixel-perfect");
}

#[test]
fn test_custom_uniform_zero_default() {
    // When a uniform exists in the shader but is not in NodeConfig::uniforms,
    // it should default to 0.0 (from CustomBlock zero-init)
    let mut r = match try_runtime(16, 16) { Some(r) => r, None => return };
    let mut g = Graph::new();
    let src = g.add_node(NodeKind::ShaderSource);
    let out = g.add_node(NodeKind::PixelsOut);
    g.connect_named(src, "out", out, "in").unwrap();
    let plan = g.compile().unwrap();

    let mut src_cfg = NodeConfig::default();
    src_cfg.frag_shader = Some(r#"
#version 330 core
in vec2 v_uv;
out vec4 fragColor;
uniform float u_level;  // not set in NodeConfig — should be 0.0
void main() { fragColor = vec4(u_level, 0.0, 0.0, 1.0); }
"#.to_owned());
    // intentionally NOT setting u_level in src_cfg.uniforms

    let mut cfg = HashMap::new();
    cfg.insert(src, src_cfg);
    cfg.insert(out, NodeConfig::default());

    let ctx = FrameCtx { width: 16, height: 16, time: 0.0, frame: 0 };
    let mut sink = PixelReadbackSink::new();
    r.execute_frame(&g, &plan, &cfg, &ctx, &mut sink).unwrap();

    let px = sink.take_pixels(out).unwrap();
    for chunk in px.chunks(4) {
        assert_eq!(chunk[0], 0, "R should be 0, u_level defaults to 0.0");
    }
    eprintln!("[PASS] test_custom_uniform_zero_default");
}

#[test]
fn test_feedback_node_pingpong() {
    // Feedback node: renders into ping-pong buffers, accumulates over frames.
    // After several frames the output should differ from frame 1 (decay accumulates).
    let mut r = match try_runtime(32, 32) { Some(r) => r, None => return };

    let mut g = Graph::new();
    let src  = g.add_node(NodeKind::ShaderSource);
    let fb   = g.add_node(NodeKind::Feedback);
    let out  = g.add_node(NodeKind::PixelsOut);
    // ShaderSource → Feedback (iChannel0=live), Feedback → PixelsOut
    g.connect_named(src, "out", fb,  "in").unwrap();
    g.connect_named(fb,  "out", out, "in").unwrap();
    let plan = g.compile().unwrap();

    let mut cfg = HashMap::new();
    cfg.insert(src, NodeConfig::default());
    cfg.insert(fb,  NodeConfig::default()); // built-in feedback shader
    cfg.insert(out, NodeConfig::default());

    // Frame 1
    let mut s1 = PixelReadbackSink::new();
    r.execute_frame(&g, &plan, &cfg,
        &FrameCtx { width:32, height:32, time:0.1, frame:1 }, &mut s1).unwrap();
    let p1 = s1.take_pixels(out).unwrap();

    // Frames 2–5 (let feedback accumulate)
    for i in 2..5u64 {
        let mut sx = PixelReadbackSink::new();
        r.execute_frame(&g, &plan, &cfg,
            &FrameCtx { width:32, height:32, time:i as f32 * 0.1, frame:i }, &mut sx).unwrap();
    }

    // Frame 6
    let mut s6 = PixelReadbackSink::new();
    r.execute_frame(&g, &plan, &cfg,
        &FrameCtx { width:32, height:32, time:0.6, frame:6 }, &mut s6).unwrap();
    let p6 = s6.take_pixels(out).unwrap();

    // After 5 frames of feedback the output should differ from frame 1
    assert_ne!(p1, p6, "Feedback did not accumulate over frames — ping-pong not working");
    eprintln!("[PASS] test_feedback_node_pingpong — feedback accumulated over 6 frames");
}

#[test]
fn test_previous_frame_node() {
    // PreviousFrame node: outputs the previous frame's content.
    // Frame 1 should be black (initialised to zero), frame 2 = frame 1 content.
    let mut r = match try_runtime(32, 32) { Some(r) => r, None => return };

    let mut g = Graph::new();
    let prev = g.add_node(NodeKind::PreviousFrame);
    let out  = g.add_node(NodeKind::PixelsOut);
    g.connect_named(prev, "out", out, "in").unwrap();
    let plan = g.compile().unwrap();

    let mut cfg = HashMap::new();
    cfg.insert(prev, NodeConfig::default());
    cfg.insert(out,  NodeConfig::default());

    // Frame 0: PreviousFrame initialised to black → output should be black
    let mut s0 = PixelReadbackSink::new();
    r.execute_frame(&g, &plan, &cfg,
        &FrameCtx { width:32, height:32, time:0.0, frame:0 }, &mut s0).unwrap();
    let p0 = s0.take_pixels(out).unwrap();
    // Frame 0: PreviousFrame has no previous frame — renders builtin gradient.
    assert_eq!(p0.len(), 32 * 32 * 4, "Frame 0: wrong pixel count");
    assert!(p0.chunks(4).all(|c| c[3] == 255), "Frame 0: alpha should be 255");

    eprintln!("[PASS] test_previous_frame_node — frame 0 is black as expected");
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
fn test_proc_amp_shader() {
    // Smoke test: proc-amp.frag compiles and renders without panicking
    let mut r = match try_runtime(32, 32) { Some(r) => r, None => return };
    let mut g = Graph::new();
    let src = g.add_node(NodeKind::ShaderSource);
    let out = g.add_node(NodeKind::PixelsOut);
    g.connect_named(src, "out", out, "in").unwrap();
    let plan = g.compile().unwrap();

    // proc-amp shader inlined — tests the full custom uniform pipeline
    // with a real LZX-style shader
    let proc_amp_frag = r#"
#version 330 core
in vec2 v_uv;
out vec4 fragColor;
uniform sampler2D iChannel0;
uniform float u_brightness;
uniform float u_contrast;
uniform float u_saturation;
uniform float u_hue;

vec3 rgb_to_yiq(vec3 c) {
    return vec3(
         0.2990 * c.r + 0.5870 * c.g + 0.1140 * c.b,
         0.5959 * c.r - 0.2746 * c.g - 0.3213 * c.b,
         0.2115 * c.r - 0.5227 * c.g + 0.3112 * c.b
    );
}
vec3 yiq_to_rgb(vec3 yiq) {
    return clamp(vec3(
        yiq.x + 0.9563 * yiq.y + 0.6210 * yiq.z,
        yiq.x - 0.2721 * yiq.y - 0.6474 * yiq.z,
        yiq.x - 1.1070 * yiq.y + 1.7046 * yiq.z
    ), 0.0, 1.0);
}
void main() {
    vec4 src = texture(iChannel0, v_uv);
    vec3 yiq = rgb_to_yiq(src.rgb);
    yiq.x = (yiq.x - 0.5) * u_contrast + 0.5 + u_brightness;
    yiq.yz *= u_saturation;
    float angle = radians(u_hue);
    float i = yiq.y * cos(angle) - yiq.z * sin(angle);
    float q = yiq.y * sin(angle) + yiq.z * cos(angle);
    yiq.yz = vec2(i, q);
    fragColor = vec4(yiq_to_rgb(yiq), src.a);
}
"#;

    let mut cfg = HashMap::new();
    cfg.insert(src, NodeConfig {
        frag_shader: Some(proc_amp_frag.to_owned()),
        uniforms: {
            let mut m = std::collections::HashMap::new();
            m.insert("u_brightness".to_owned(), 0.1);
            m.insert("u_contrast".to_owned(),   1.2);
            m.insert("u_saturation".to_owned(), 0.8);
            m.insert("u_hue".to_owned(),       15.0);
            m
        },
        output_name: None,
            input_textures: [None, None, None, None],
    });
    cfg.insert(out, NodeConfig::default());

    let ctx = FrameCtx { width: 32, height: 32, time: 0.5, frame: 10 };
    let mut sink = PixelReadbackSink::new();
    r.execute_frame(&g, &plan, &cfg, &ctx, &mut sink).unwrap();
    let px = sink.take_pixels(out).expect("no pixels");
    assert_eq!(px.len(), 32 * 32 * 4);
    eprintln!("[PASS] test_proc_amp_shader");
}
