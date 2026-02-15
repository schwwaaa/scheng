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
    /// UDP bind address for OSC.
    bind_addr: String,
}

fn print_usage_and_exit() -> ! {
    eprintln!(
        "Usage:\n  scheng-example-graph-matrix-mix4-webcam [--bind 127.0.0.1:9000]\n\n\
         OSC params (via /param/...):\n  /param/w0 0.0..1.0  (mix weight for input 0)\n  /param/w1\n  /param/w2\n  /param/w3\n"
    );
    std::process::exit(2);
}

fn parse_args() -> AppConfig {
    let mut args = std::env::args().skip(1);
    let mut bind_addr: Option<String> = None;

    while let Some(a) = args.next() {
        match a.as_str() {
            "--bind" => bind_addr = args.next(),
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

impl Presenter {
    unsafe fn new(gl: &glow::Context) -> Result<Self, rt::EngineError> {
        // Fragment shader in external file.
        let frag_src = include_str!("../shader_present.glsl");
        let tri = rt::FullscreenTriangle::new(gl)?;
        let program = rt::compile_program(gl, rt::FULLSCREEN_VERT, frag_src)?;
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

// make_gl copied from webcam_source_minimal, with only title changed.
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
            .with_title("scheng: graph_matrix_mix4_webcam")
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

fn open_any_webcam() -> Option<Webcam> {
    // Try a few common indices; first successful one wins.
    for idx in 0..4u32 {
        match Webcam::new(idx, 640, 480) {
            Ok(cam) => {
                eprintln!("Opened webcam at index {idx}");
                return Some(cam);
            }
            Err(e) => {
                eprintln!("Webcam index {idx} failed: {e}");
            }
        }
    }

    eprintln!("No webcams opened for indices 0..3; continuing without live input.");
    None
}


fn main() {
    let cfg = parse_args();

    let mut osc = match OscParamReceiver::bind(&cfg.bind_addr) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Failed to bind OSC on {}: {e}", cfg.bind_addr);
            std::process::exit(1);
        }
    };

    println!("OSC listening on {}", cfg.bind_addr);
    println!("Send /param/w0..w3 to control mix weights (0.0–1.0).");

    let event_loop = winit::event_loop::EventLoop::new();
    let (window, gl_surface, gl_context, gl) = make_gl(&event_loop);

    // --- Graph ---
    let mut g = graph::Graph::new();

    // Webcam branch: TextureInputPass -> ShaderPass (webcam passthrough).
    let tex_in = g.add_node(graph::NodeKind::TextureInputPass);
    let p0 = g.add_node(graph::NodeKind::ShaderPass);
    let p1 = g.add_node(graph::NodeKind::ShaderPass);
    let p2 = g.add_node(graph::NodeKind::ShaderPass);
    let p3 = g.add_node(graph::NodeKind::ShaderPass);

    let mix = g.add_node(graph::NodeKind::MatrixMix4);
    let out = g.add_node(graph::NodeKind::PixelsOut);

    // PortId order semantics: in0..in3
    g.connect_named(p0, "out", mix, "in0").unwrap();
    g.connect_named(p1, "out", mix, "in1").unwrap();
    g.connect_named(p2, "out", mix, "in2").unwrap();
    g.connect_named(p3, "out", mix, "in3").unwrap();
    g.connect_named(mix, "out", out, "in").unwrap();

    // Webcam texture into p3.
    g.connect_named(tex_in, "out", p3, "in").unwrap();

    let plan = g.compile().expect("compile plan");

    // --- Node properties ---
    let mut props = rt::NodeProps::default();

    // Attach shaders directly to ShaderPass nodes from external files.
    props.shader_sources.insert(
        p0,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: include_str!("../shader_r.glsl").to_string(),
            origin: Some("graph_matrix_mix4_webcam:p0".to_string()),
        },
    );
    props.shader_sources.insert(
        p1,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: include_str!("../shader_g.glsl").to_string(),
            origin: Some("graph_matrix_mix4_webcam:p1".to_string()),
        },
    );
    props.shader_sources.insert(
        p2,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: include_str!("../shader_b.glsl").to_string(),
            origin: Some("graph_matrix_mix4_webcam:p2".to_string()),
        },
    );
    props.shader_sources.insert(
        p3,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: include_str!("../shader_webcam_pass.glsl").to_string(),
            origin: Some("graph_matrix_mix4_webcam:p3_webcam".to_string()),
        },
    );

    // Initial 4-way equal mix; will be overridden by OSC each frame.
    props.matrix_params.insert(
        mix,
        MatrixMixParams {
            weights: [0.25, 0.25, 0.25, 0.25],
        },
    );

    let mut state = unsafe { rt::RuntimeState::new(&gl).expect("rt state") };
    let presenter = unsafe { Presenter::new(&gl).expect("presenter") };

    // --- Webcam (optional, auto-detect index 0..3) ---
    let mut cam_opt: Option<Webcam> = open_any_webcam();

    let mut tex_w: i32 = 640;
    let mut tex_h: i32 = 480;
    let mut host_tex = unsafe { make_host_texture(&gl, tex_w, tex_h) };

    let t0 = Instant::now();
    let mut frame_index: u64 = 0;

    // OSC-controlled weights for MatrixMix4 inputs.
    let mut w0: f32 = 0.25;
    let mut w1: f32 = 0.25;
    let mut w2: f32 = 0.25;
    let mut w3: f32 = 0.25;

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
                // Drain OSC updates once per frame; library strips /param/ prefix.
                for (name, val) in osc.poll() {
                    match name.as_str() {
                        "w0" => w0 = val,
                        "w1" => w1 = val,
                        "w2" => w2 = val,
                        "w3" => w3 = val,
                        _ => {}
                    }
                }

                // Update matrix weights in NodeProps.
                if let Some(mp) = props.matrix_params.get_mut(&mix) {
                    mp.weights = [w0, w1, w2, w3];
                }

                let elapsed = t0.elapsed().as_secs_f32();

                // Pull a frame from the webcam if available, updating the host texture.
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

                // Bind the host texture (webcam or empty) into the TextureInputPass node.
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
                    let primary = outs.primary;

                    presenter.present(&gl, primary.tex, w, h);
                    gl_surface.swap_buffers(&gl_context).unwrap();
                }
            }
            _ => {}
        }
    });
}
/*  */