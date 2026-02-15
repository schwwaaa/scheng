// examples/static_source_minimal/src/main.rs
//
// Step 11.2.1: Static source (zero external deps)
//
// This example creates a host GL texture once (a static checkerboard), feeds it
// into `NodeKind::TextureInputPass` (a Source node), then routes to `PixelsOut`
// and presents to the window.
//
// Purpose:
// - Harden the Source -> downstream consumption path with a deterministic input.
// - Exercise TextureInputPass as a *true Source* (no shader, no program cache).

use std::num::NonZeroU32;

use glow::HasContext;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextAttributesBuilder, NotCurrentGlContextSurfaceAccessor, PossiblyCurrentContext};
use glutin::display::GetGlDisplay;
use glutin::prelude::{GlConfig, GlDisplay, GlSurface};
use glutin_winit::DisplayBuilder;
use raw_window_handle::{HasRawWindowHandle, HasRawDisplayHandle};
use winit::dpi::PhysicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::ControlFlow;

use scheng_graph as graph;
use scheng_runtime_glow as rt;

const WIN_W: u32 = 960;
const WIN_H: u32 = 540;

// Simple fullscreen blit shader to present a texture to the default framebuffer.
const PRESENT_FRAG: &str = r#"#version 330 core
in vec2 v_uv;
out vec4 o;
uniform sampler2D iChannel0;
void main() { o = texture(iChannel0, v_uv); }
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
    let template = ConfigTemplateBuilder::new()
        .with_alpha_size(8)
        .with_depth_size(0)
        .with_stencil_size(0);

    let display_builder = DisplayBuilder::new().with_window_builder(Some(
        winit::window::WindowBuilder::new()
            .with_title("scheng: static_source_minimal")
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
    let _raw_display_handle = window.raw_display_handle();

    let gl_display = gl_config.display();

    let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window_handle));
    let not_current_gl_context = unsafe { gl_display.create_context(&gl_config, &context_attributes).unwrap() };

    let size = window.inner_size();
    let attrs = glutin::surface::SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new().build(
        raw_window_handle,
        NonZeroU32::new(size.width.max(1)).unwrap(),
        NonZeroU32::new(size.height.max(1)).unwrap(),
    );

    let gl_surface = unsafe { gl_display.create_window_surface(&gl_config, &attrs).unwrap() };
    let gl_context = not_current_gl_context.make_current(&gl_surface).unwrap();

    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            gl_display.get_proc_address(
                std::ffi::CStr::from_bytes_with_nul_unchecked(format!("{s}\0").as_bytes()),
            ) as *const _
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

unsafe fn upload_checkerboard(gl: &glow::Context, tex: glow::NativeTexture, w: i32, h: i32) {
    // Deterministic checkerboard with a subtle gradient so orientation is obvious.
    let mut buf = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let c = (((x / 32) ^ (y / 32)) & 1) as u8;
            let gx = (x * 255 / (w.max(1))) as u8;
            let gy = (y * 255 / (h.max(1))) as u8;

            let (r, g, b) = if c == 0 {
                (gx, 32u8, gy)
            } else {
                (32u8, gy, gx)
            };

            buf[i] = r;
            buf[i + 1] = g;
            buf[i + 2] = b;
            buf[i + 3] = 255;
        }
    }

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
        glow::PixelUnpackData::Slice(&buf),
    );
}

fn main() {
    let event_loop = winit::event_loop::EventLoop::new();
    let (window, gl_surface, gl_context, gl) = make_gl(&event_loop);

    // Graph (v1 invariant): TextureInputPass (Source) -> ShaderPass (render) -> PixelsOut (Output)
    let mut g = graph::Graph::new();
    let tex_in = g.add_node(graph::NodeKind::TextureInputPass);
    let pass = g.add_node(graph::NodeKind::ShaderPass);
    let out = g.add_node(graph::NodeKind::PixelsOut);

    // Use named ports; PortId values are global and not stable/obvious.
    g.connect_named(tex_in, "out", pass, "in").expect("connect tex->pass");
    g.connect_named(pass, "out", out, "in").expect("connect pass->out");

    let plan = g.compile().expect("compile plan");

    let mut props = rt::NodeProps::default();
    props.output_names.insert(out, "preview".into());

    props.shader_sources.insert(
        pass,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: PRESENT_FRAG.to_string(),
            origin: Some("static_source_minimal:passthrough".into()),
        },
    );

    let mut state = unsafe { rt::RuntimeState::new(&gl).expect("rt state") };
    let presenter = unsafe { Presenter::new(&gl).expect("presenter") };

    // Create and upload the static texture once.
    let tex_w = 512i32;
    let tex_h = 512i32;
    let host_tex = unsafe { make_host_texture(&gl, tex_w, tex_h) };
    unsafe { upload_checkerboard(&gl, host_tex, tex_w, tex_h) };

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
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                // Publish the texture as the source output.
                props.texture_inputs.insert(tex_in, host_tex);

                let size = window.inner_size();
                let w = size.width as i32;
                let h = size.height as i32;
                let frame = rt::FrameCtx { width: w, height: h, time: 0.0, frame: 0 };

                unsafe {
                    let outs = rt::execute_plan_outputs(&gl, &g, &plan, &mut state, &props, frame)
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
