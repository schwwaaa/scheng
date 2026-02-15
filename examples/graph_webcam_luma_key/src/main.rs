use std::num::NonZeroU32;
use std::time::Instant;

use glow::HasContext;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{
    ContextAttributesBuilder, NotCurrentGlContextSurfaceAccessor, PossiblyCurrentContext,
};
use glutin::display::GetGlDisplay;
use glutin::prelude::{GlConfig, GlDisplay, GlSurface};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasRawWindowHandle;
use winit::dpi::PhysicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::ControlFlow;

use scheng_graph as graph;
use scheng_runtime::MatrixMixParams;
use scheng_runtime_glow as rt;

use scheng_input_webcam::Webcam;
use scheng_control_osc::OscParamReceiver;

const WIN_W: u32 = 960;
const WIN_H: u32 = 540;

/// CLI config for OSC.
struct AppConfig {
    bind_addr: String,
}

fn print_usage_and_exit() -> ! {
    eprintln!(
        "Usage:\n  scheng-example-graph-webcam-luma-key [--bind 127.0.0.1:9000]\n\n\
         OSC parameters (send as /param/...):\n  /param/w0 or /param/key_low   (0.0–1.0)\n  /param/w1 or /param/key_high  (0.0–1.0)\n  /param/w2 or /param/cam_gain  (0.0–4.0)\n  /param/w3 or /param/bg_gain   (0.0–4.0)\n"
    );
    std::process::exit(2);
}

fn parse_args() -> AppConfig {
    let mut args = std::env::args().skip(1);
    let mut bind_addr: Option<String> = None;

    while let Some(a) = args.next() {
        match a.as_str() {
            "--bind" => {
                bind_addr = args.next();
            }
            "--help" | "-h" => print_usage_and_exit(),
            _ => {
                eprintln!("Unknown arg: {a}");
                print_usage_and_exit();
            }
        }
    }

    AppConfig {
        bind_addr: bind_addr.unwrap_or_else(|| "127.0.0.1:9000".to_string()),
    }
}

struct Presenter {
    tri: rt::FullscreenTriangle,
    program: glow::NativeProgram,
}

// Presenter shader: 180° rotate / flip for display.
const PRESENT_FRAG: &str = r#"#version 330 core
in vec2 v_uv;
out vec4 o;
uniform sampler2D iChannel0;
void main() {
    vec2 uv = vec2(1.0 - v_uv.x, 1.0 - v_uv.y);
    o = texture(iChannel0, uv);
}
"#;

impl Presenter {
    unsafe fn new(gl: &glow::Context) -> Result<Self, rt::EngineError> {
        let tri = rt::FullscreenTriangle::new(gl)?;
        let program = rt::compile_program(gl, rt::FULLSCREEN_VERT, PRESENT_FRAG)?;
        Ok(Self { tri, program })
    }

    unsafe fn present(&self, gl: &glow::Context, tex: glow::NativeTexture, w: i32, h: i32) {
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.viewport(0, 0, w, h);
        gl.disable(glow::DEPTH_TEST);
        gl.disable(glow::BLEND);

        gl.use_program(Some(self.program));

        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        if let Some(loc) = gl.get_uniform_location(self.program, "iChannel0") {
            gl.uniform_1_i32(Some(&loc), 0);
        }

        self.tri.draw(gl);
    }
}

// --- GL / window bootstrap (based on webcam_source_minimal) ---

fn make_gl(
    event_loop: &winit::event_loop::EventLoop<()>,
) -> (
    winit::window::Window,
    glutin::surface::Surface<glutin::surface::WindowSurface>,
    PossiblyCurrentContext,
    glow::Context,
) {
    let template =
        ConfigTemplateBuilder::new().with_alpha_size(8).with_depth_size(0).with_stencil_size(0);

    let display_builder = DisplayBuilder::new().with_window_builder(Some(
        winit::window::WindowBuilder::new()
            .with_title("scheng: webcam luma key (matrix mix4 + OSC)")
            .with_inner_size(PhysicalSize::new(WIN_W, WIN_H)),
    ));

    let (window, gl_config) = display_builder
        .build(event_loop, template, |configs| {
            configs
                .reduce(|a, b| if a.num_samples() > b.num_samples() { a } else { b })
                .unwrap()
        })
        .unwrap();

    let window = window.unwrap();
    let raw_window_handle = window.raw_window_handle();

    let gl_display = gl_config.display();

    let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window_handle));
    let not_current_gl_context =
        unsafe { gl_display.create_context(&gl_config, &context_attributes).unwrap() };

    let size = window.inner_size();
    let attrs = glutin::surface::SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new()
        .build(
            raw_window_handle,
            NonZeroU32::new(size.width.max(1)).unwrap(),
            NonZeroU32::new(size.height.max(1)).unwrap(),
        );

    let gl_surface = unsafe { gl_display.create_window_surface(&gl_config, &attrs).unwrap() };

    let gl_context = not_current_gl_context.make_current(&gl_surface).unwrap();

    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            gl_display.get_proc_address(std::ffi::CStr::from_bytes_with_nul_unchecked(
                format!("{s}\0").as_bytes(),
            )) as *const _
        })
    };

    (window, gl_surface, gl_context, gl)
}

unsafe fn make_host_texture(gl: &glow::Context, w: i32, h: i32) -> glow::NativeTexture {
    let tex = gl.create_texture().unwrap();
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA8 as i32,
        w,
        h,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        None,
    );
    tex
}

unsafe fn upload_rgba_to_texture(
    gl: &glow::Context,
    tex: glow::NativeTexture,
    w: i32,
    h: i32,
    rgba: &[u8],
) {
    debug_assert_eq!(rgba.len(), (w * h * 4) as usize);
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_sub_image_2d(
        glow::TEXTURE_2D,
        0,
        0,
        0,
        w,
        h,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(rgba),
    );
}

// Try indices 0..3 to find any webcam; optional like before.
fn open_any_webcam() -> Option<Webcam> {
    for idx in 0..4u32 {
        match Webcam::new(idx, 640, 480) {
            Ok(cam) => {
                eprintln!("[webcam] opened at index {idx}");
                return Some(cam);
            }
            Err(e) => {
                eprintln!("[webcam] index {idx} failed: {e}");
            }
        }
    }

    eprintln!("[webcam] no webcams opened for indices 0..3; continuing without live input.");
    None
}

fn main() {
    let cfg = parse_args();

    let mut osc = OscParamReceiver::bind(&cfg.bind_addr).unwrap_or_else(|e| {
        eprintln!("[osc] failed to bind on {}: {e}", cfg.bind_addr);
        std::process::exit(1);
    });

    println!("--- webcam luma key + OSC example ---");
    println!("OSC listening on {}", cfg.bind_addr);
    println!("Addresses (either name is accepted):");
    println!("  /param/w0        or /param/key_low");
    println!("  /param/w1        or /param/key_high");
    println!("  /param/w2        or /param/cam_gain");
    println!("  /param/w3        or /param/bg_gain");

    let event_loop = winit::event_loop::EventLoop::new();
    let (window, gl_surface, gl_context, gl) = make_gl(&event_loop);

    // --- Graph: webcam -> pass -> mix4(key) + 3 GLSL layers -> PixelsOut ---

    let mut g = graph::Graph::new();

    let tex_in = g.add_node(graph::NodeKind::TextureInputPass);
    let webcam_pass = g.add_node(graph::NodeKind::ShaderPass);

    let layer_a = g.add_node(graph::NodeKind::ShaderPass);
    let layer_b = g.add_node(graph::NodeKind::ShaderPass);
    let layer_c = g.add_node(graph::NodeKind::ShaderPass);

    let mix = g.add_node(graph::NodeKind::MatrixMix4);
    let out = g.add_node(graph::NodeKind::PixelsOut);

    // Webcam texture into simple ShaderPass, then into mix input 0.
    g.connect_named(tex_in, "out", webcam_pass, "in")
        .expect("connect tex->webcam_pass");

    // MatrixMix4 ports: in0..in3
    g.connect_named(webcam_pass, "out", mix, "in0")
        .expect("connect webcam->mix0");
    g.connect_named(layer_a, "out", mix, "in1")
        .expect("connect layer_a->mix1");
    g.connect_named(layer_b, "out", mix, "in2")
        .expect("connect layer_b->mix2");
    g.connect_named(layer_c, "out", mix, "in3")
        .expect("connect layer_c->mix3");

    g.connect_named(mix, "out", out, "in")
        .expect("connect mix->out");

    let plan = g.compile().expect("compile plan");

    // --- Runtime props + shaders ---

    let mut props = rt::NodeProps::default();
    props.output_names.insert(out, "preview".into());

    // Webcam passthrough (no rotation here; presenter does that).
    props.shader_sources.insert(
        webcam_pass,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: include_str!("../shaders/webcam_passthrough.frag").to_string(),
            origin: Some("graph_webcam_luma_key:webcam_pass".into()),
        },
    );

    // Three independent GLSL layers (each a distinct form).
    props.shader_sources.insert(
        layer_a,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: include_str!("../shaders/layer_a.frag").to_string(),
            origin: Some("graph_webcam_luma_key:layer_a".into()),
        },
    );
    props.shader_sources.insert(
        layer_b,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: include_str!("../shaders/layer_b.frag").to_string(),
            origin: Some("graph_webcam_luma_key:layer_b".into()),
        },
    );
    props.shader_sources.insert(
        layer_c,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: include_str!("../shaders/layer_c.frag").to_string(),
            origin: Some("graph_webcam_luma_key:layer_c".into()),
        },
    );

    // Override MatrixMix4’s default mixer with our luma key shader (uses uWeights).
    props.shader_sources.insert(
        mix,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: include_str!("../shaders/webcam_luma_key_mix4.frag").to_string(),
            origin: Some("graph_webcam_luma_key:mix4_keyer".into()),
        },
    );

    // Initial weights: [key_low, key_high, cam_gain, bg_gain]
    let mut w0: f32 = 0.2;
    let mut w1: f32 = 0.8;
    let mut w2: f32 = 1.0;
    let mut w3: f32 = 1.0;

    props.matrix_params.insert(
        mix,
        MatrixMixParams {
            weights: [w0, w1, w2, w3],
        },
    );

    let mut state = unsafe { rt::RuntimeState::new(&gl).expect("rt state") };
    let presenter = unsafe { Presenter::new(&gl).expect("presenter") };

    // --- Webcam host texture (optional) ---

    let mut cam_opt: Option<Webcam> = open_any_webcam();

    let mut tex_w: i32 = 640;
    let mut tex_h: i32 = 480;
    let mut host_tex = unsafe { make_host_texture(&gl, tex_w, tex_h) };

    let t0 = Instant::now();
    let mut frame_index: u64 = 0;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                WindowEvent::Resized(size) => {
                    gl_surface.resize(
                        &gl_context,
                        NonZeroU32::new(size.width.max(1)).unwrap(),
                        NonZeroU32::new(size.height.max(1)).unwrap(),
                    );
                }
                _ => {}
            },
            Event::MainEventsCleared => window.request_redraw(),
            Event::RedrawRequested(_) => {
                // ---- OSC: update weights, log everything we see ----
                let mut any_osc = false;
                for (name, val) in osc.poll() {
                    any_osc = true;
                    println!("[osc] received {name} = {val}");

                    match name.as_str() {
                        // matrix-style names
                        "w0" | "key_low" => {
                            w0 = val.clamp(0.0, 1.0);
                        }
                        "w1" | "key_high" => {
                            w1 = val.clamp(0.0, 1.0);
                        }
                        "w2" | "cam_gain" => {
                            w2 = val.clamp(0.0, 4.0);
                        }
                        "w3" | "bg_gain" => {
                            w3 = val.clamp(0.0, 4.0);
                        }
                        _ => {
                            println!("[osc] unknown param '{name}', ignoring");
                        }
                    }
                }

                if any_osc {
                    println!(
                        "[osc] applied -> weights = [{:.3}, {:.3}, {:.3}, {:.3}]",
                        w0, w1, w2, w3
                    );
                }

                if let Some(p) = props.matrix_params.get_mut(&mix) {
                    p.weights = [w0, w1, w2, w3];
                }

                let elapsed = t0.elapsed().as_secs_f32();

                // ---- Webcam: update host texture if camera is available ----
                if let Some(cam) = cam_opt.as_mut() {
                    if let Ok(frame) = cam.poll_rgba() {
                        let w = frame.width as i32;
                        let h = frame.height as i32;

                        if w != tex_w || h != tex_h {
                            unsafe { gl.delete_texture(host_tex) };
                            tex_w = w;
                            tex_h = h;
                            host_tex = unsafe { make_host_texture(&gl, tex_w, tex_h) };
                        }

                        unsafe {
                            upload_rgba_to_texture(&gl, host_tex, tex_w, tex_h, &frame.bytes);
                        }
                    }
                }

                // Provide webcam (or empty) texture to TextureInputPass node.
                props.texture_inputs.insert(tex_in, host_tex);

                let size = window.inner_size();
                let w = size.width as i32;
                let h = size.height as i32;

                let frame = rt::FrameCtx {
                    width: w,
                    height: h,
                    time: elapsed,
                    frame: frame_index,
                };
                frame_index = frame_index.wrapping_add(1);

                unsafe {
                    let outs =
                        rt::execute_plan_outputs(&gl, &g, &plan, &mut state, &props, frame)
                            .expect("execute");
                    let main_out = outs.primary;

                    presenter.present(&gl, main_out.tex, w, h);
                    gl_surface.swap_buffers(&gl_context).unwrap();
                }
            }
            _ => {}
        }
    });
}
