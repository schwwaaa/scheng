use std::num::NonZeroU32;
use std::time::Instant;

use glow::HasContext;

use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, NotCurrentContext, Version};
use glutin::context::NotCurrentGlContextSurfaceAccessor;
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::prelude::GlSurface;
use glutin::surface::{SurfaceAttributesBuilder, SwapInterval, WindowSurface};

use glutin_winit::DisplayBuilder;
use raw_window_handle::HasRawWindowHandle;
use winit::dpi::PhysicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

use scheng_graph as graph;
use scheng_runtime_glow as rt;

// IMPORTANT: bring trait into scope for `.consume()`
use rt::OutputSink;

const RENDER_W: i32 = 1920;
const RENDER_H: i32 = 1080;

// This example is intentionally "patchbay-like" without relying on any PatchbaySink
// internals (so it can't break on private fields).
//
// It demonstrates routing the same rendered output to:
//   1) Syphon (for VJ apps)
//   2) The local preview window (blit)

struct PresentBlitSink {
    w: i32,
    h: i32,
}

impl rt::OutputSink for PresentBlitSink {
    fn consume(&mut self, gl: &glow::Context, out: &rt::ExecOutput) {
        unsafe {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(out.fbo));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
            gl.blit_framebuffer(
                0,
                0,
                out.width,
                out.height,
                0,
                0,
                self.w,
                self.h,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
        }
    }
}

fn build_graph(props: &mut rt::NodeProps) -> graph::Graph {
    let mut g = graph::Graph::default();

    let pass = g.add_node(graph::NodeKind::ShaderPass);
    let out = g.add_node(graph::NodeKind::PixelsOut);

    g.connect_named(pass, "out", out, "in").unwrap();

    props.shader_sources.insert(
        pass,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: include_str!("../shader_frag.glsl").to_string(),
            origin: Some("syphon_patchbay".to_string()),
        },
    );

    g
}

fn main() {
    let event_loop = EventLoop::new();

    let template = ConfigTemplateBuilder::new().with_alpha_size(8);
    let display_builder = DisplayBuilder::new().with_window_builder(Some(
        WindowBuilder::new()
            .with_title("scheng syphon patchbay")
            .with_inner_size(PhysicalSize::new(RENDER_W as u32, RENDER_H as u32)),
    ));

    let (window, gl_config) = display_builder
        .build(&event_loop, template, |mut configs| configs.next().unwrap())
        .unwrap();

    let window = window.unwrap();

    let raw_window_handle = window.raw_window_handle();
    let gl_display = gl_config.display();

    let context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .build(Some(raw_window_handle));

    let fallback_context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::Gles(Some(Version::new(3, 0))))
        .build(Some(raw_window_handle));

    let not_current_gl_context: NotCurrentContext = unsafe {
        gl_display
            .create_context(&gl_config, &context_attributes)
            .unwrap_or_else(|_| {
                gl_display
                    .create_context(&gl_config, &fallback_context_attributes)
                    .expect("failed to create gl context")
            })
    };

    let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        raw_window_handle,
        NonZeroU32::new(RENDER_W as u32).unwrap(),
        NonZeroU32::new(RENDER_H as u32).unwrap(),
    );

    let gl_surface = unsafe { gl_display.create_window_surface(&gl_config, &attrs).unwrap() };

    let gl_context = not_current_gl_context.make_current(&gl_surface).unwrap();

    gl_surface
        .set_swap_interval(&gl_context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()))
        .ok();

    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            let cstr = std::ffi::CString::new(s).unwrap();
            gl_display.get_proc_address(&cstr).cast()
        })
    };

    let mut props = rt::NodeProps::default();
    let g = build_graph(&mut props);
    let plan = g.compile().expect("compile plan");

    let mut state = unsafe { rt::RuntimeState::new(&gl) }.expect("runtime state");

    let mut present = PresentBlitSink {
        w: RENDER_W,
        h: RENDER_H,
    };

    let mut syphon = rt::SyphonSink::new("scheng").expect("syphon init");

    let t0 = Instant::now();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                WindowEvent::Resized(size) => {
                    present.w = size.width as i32;
                    present.h = size.height as i32;
                    gl_surface.resize(
                        &gl_context,
                        NonZeroU32::new(size.width.max(1)).unwrap(),
                        NonZeroU32::new(size.height.max(1)).unwrap(),
                    );
                }
                _ => {}
            },
            Event::MainEventsCleared => {
                let time = t0.elapsed().as_secs_f32();

                let frame = rt::FrameCtx {
                    width: RENDER_W,
                    height: RENDER_H,
                    time,
                    frame: 0,
                };

                let outs = unsafe {
                    rt::execute_plan_outputs(&gl, &g, &plan, &mut state, &props, frame)
                        .expect("execute")
                };

                let main_out = outs.get(rt::OUTPUT_MAIN).unwrap_or(outs.primary());

                // "patchbay": route the same output to multiple sinks
                syphon.consume(&gl, main_out);
                present.consume(&gl, main_out);

                gl_surface.swap_buffers(&gl_context).unwrap();
            }
            _ => {}
        }
    });
}
