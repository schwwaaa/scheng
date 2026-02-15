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
use scheng_runtime_glow as rt;

use scheng_input_webcam::Webcam;

const WIN_W: u32 = 960;
const WIN_H: u32 = 540;

// Graph pass shader: passthrough ONLY.
// (Do NOT rotate here, or you'll cancel out with the presenter.)
const PASS_FRAG: &str = r#"#version 330 core
in vec2 v_uv;
out vec4 o;
uniform sampler2D iChannel0;
void main() { o = texture(iChannel0, v_uv); }
"#;

// Presenter shader: apply rotation/flip for display.
// 180° rotation = uv -> 1 - uv
const PRESENT_FRAG: &str = r#"#version 330 core
in vec2 v_uv;
out vec4 o;
uniform sampler2D iChannel0;
void main() {
    vec2 uv = vec2(1.0 - v_uv.x, 1.0 - v_uv.y);
    o = texture(iChannel0, uv);
}
"#;

struct Presenter {
    tri: rt::FullscreenTriangle,
    program: glow::NativeProgram,
}

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
            .with_title("scheng: webcam_source_minimal (Step 11.2.3)")
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

fn main() {
    let event_loop = winit::event_loop::EventLoop::new();
    let (window, gl_surface, gl_context, gl) = make_gl(&event_loop);

    // --- Graph ---
    let mut g = graph::Graph::new();
    let tex_in = g.add_node(graph::NodeKind::TextureInputPass);
    let pass = g.add_node(graph::NodeKind::ShaderPass);
    let out = g.add_node(graph::NodeKind::PixelsOut);

    g.connect_named(tex_in, "out", pass, "in").expect("connect tex->pass");
    g.connect_named(pass, "out", out, "in").expect("connect pass->out");

    let plan = g.compile().expect("compile plan");

    let mut props = rt::NodeProps::default();
    props.output_names.insert(out, "preview".into());

    // Use PASS_FRAG here (no flip).
    props.shader_sources.insert(
        pass,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: PASS_FRAG.to_string(),
            origin: Some("webcam_source_minimal:passthrough".into()),
        },
    );

    let mut state = unsafe { rt::RuntimeState::new(&gl).expect("rt state") };
    let presenter = unsafe { Presenter::new(&gl).expect("presenter") };

    // --- Webcam ---
    let mut cam = Webcam::new(0, 640, 480).expect("open webcam");

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
                let elapsed = t0.elapsed().as_secs_f32();

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

                    // Presenter applies the 180° rotation.
                    presenter.present(&gl, main_out.tex, w, h);
                    gl_surface.swap_buffers(&gl_context).unwrap();
                }
            }
            _ => {}
        }
    });
}
