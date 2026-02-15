use glow::HasContext;
use scheng_graph::{Graph, NodeKind};
use scheng_runtime_glow::{
    execute_plan_to_sink, EngineError, FrameCtx, NodeProps, OutputSink, RuntimeState, ShaderSource,
    FULLSCREEN_VERT,
};

struct PresentBlitSink {
    w: i32,
    h: i32,
}

impl OutputSink for PresentBlitSink {
    fn consume(&mut self, gl: &glow::Context, out: &scheng_runtime_glow::ExecOutput) {
        unsafe {
            // Blit the plan output into the default framebuffer.
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
                glow::LINEAR,
            );
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
        }
    }
}

use std::num::NonZeroU32;
use std::time::Instant;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

use glutin::display::GetGlDisplay;
use glutin::prelude::*;

// raw-window-handle 0.5 traits (matches glutin 0.30)
use raw_window_handle::HasRawWindowHandle;

fn main() {
    if let Err(e) = run() {
        eprintln!("[scheng-sdk example] error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), EngineError> {
    // --- Build a minimal graph: ShaderSource -> ShaderPass -> PixelsOut
    let mut graph = Graph::new();
    let src = graph.add_node(NodeKind::ShaderSource);
    let pass = graph.add_node(NodeKind::ShaderPass);
    let out = graph.add_node(NodeKind::PixelsOut);

    graph.connect_named(src, "out", pass, "in")?;
    graph.connect_named(pass, "out", out, "in")?;

    let plan = graph.compile()?;

    // Runtime-side properties (shader code lives here, not in the graph).
    let mut props = NodeProps::default();
    props.shader_sources.insert(
        src,
        ShaderSource {
            vert: FULLSCREEN_VERT.to_string(),
            frag: r#"
#version 330 core
in vec2 v_uv;
out vec4 fragColor;
uniform float u_time;
uniform vec2  u_resolution;
void main() {
    vec2 uv01 = clamp(v_uv * 0.5, 0.0, 1.0);
    float t = 0.5 + 0.5*sin(u_time);
    fragColor = vec4(uv01.x, uv01.y, t, 1.0);
}
"#
            .to_string(),
            origin: Some("inline".into()),
        },
    );

    // --- Window + GL context
    let event_loop = EventLoop::new();

    let window_builder = WindowBuilder::new()
        .with_title("scheng-sdk: graph_minimal (C3 pull-based)")
        .with_inner_size(winit::dpi::LogicalSize::new(960.0, 540.0));

    let template = glutin::config::ConfigTemplateBuilder::new()
        .with_alpha_size(8)
        .with_depth_size(0)
        .with_stencil_size(0)
        .with_transparency(false);

    let display_builder =
        glutin_winit::DisplayBuilder::new().with_window_builder(Some(window_builder));

    let (window, gl_config) = display_builder
        .build(&event_loop, template, |configs| {
            configs
                .reduce(|accum, config| {
                    if config.num_samples() > accum.num_samples() {
                        config
                    } else {
                        accum
                    }
                })
                .unwrap()
        })
        .map_err(|e| EngineError::GlCreate(format!("DisplayBuilder.build: {e}")))?;

    let window = window
        .ok_or_else(|| EngineError::GlCreate("DisplayBuilder did not create a window".into()))?;
    let gl_display = gl_config.display();

    let raw_window_handle = window.raw_window_handle();

    let context_attributes = glutin::context::ContextAttributesBuilder::new()
        .with_profile(glutin::context::GlProfile::Core)
        .build(Some(raw_window_handle));

    let fallback_context_attributes = glutin::context::ContextAttributesBuilder::new()
        .with_profile(glutin::context::GlProfile::Core)
        .build(None);

    let not_current_gl_context = unsafe {
        gl_display
            .create_context(&gl_config, &context_attributes)
            .or_else(|_| gl_display.create_context(&gl_config, &fallback_context_attributes))
            .map_err(|e| EngineError::GlCreate(format!("create_context: {e}")))?
    };

    let (width, height) = {
        let s = window.inner_size();
        (s.width.max(1), s.height.max(1))
    };

    let attrs = glutin::surface::SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new()
        .build(
            raw_window_handle,
            NonZeroU32::new(width).unwrap(),
            NonZeroU32::new(height).unwrap(),
        );

    let gl_surface = unsafe {
        gl_display
            .create_window_surface(&gl_config, &attrs)
            .map_err(|e| EngineError::GlCreate(format!("create_window_surface: {e}")))?
    };

    let gl_context = not_current_gl_context
        .make_current(&gl_surface)
        .map_err(|e| EngineError::GlCreate(format!("make_current: {e}")))?;

    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            gl_display.get_proc_address(std::ffi::CString::new(s).unwrap().as_c_str()) as *const _
        })
    };

    let start = Instant::now();
    let mut state = unsafe { RuntimeState::new(&gl)? };

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    unsafe { state.destroy(&gl) };
                    *control_flow = ControlFlow::Exit;
                }

                WindowEvent::Resized(physical_size) => {
                    let w = physical_size.width.max(1);
                    let h = physical_size.height.max(1);
                    gl_surface.resize(
                        &gl_context,
                        NonZeroU32::new(w).unwrap(),
                        NonZeroU32::new(h).unwrap(),
                    );
                    window.request_redraw();
                }

                _ => {}
            },

            Event::MainEventsCleared => window.request_redraw(),

            Event::RedrawRequested(_) => {
                let (w, h) = {
                    let s = window.inner_size();
                    (s.width.max(1) as i32, s.height.max(1) as i32)
                };

                let frame = FrameCtx {
                    time: start.elapsed().as_secs_f32(),
                    width: w,
                    height: h,
                    frame: 0,
                };

                // Pull one frame through the Plan.
                let mut sink = PresentBlitSink {
                    w,
                    h,
                };
                let exec = unsafe {
                    execute_plan_to_sink(&gl, &graph, &plan, &mut state, &props, frame, &mut sink)
                };
                match exec {
                    Ok(_res) => { /* presented via sink */ }
                    Err(e) => {
                        eprintln!("execute_plan error: {e}");
                    }
                }

                gl_surface.swap_buffers(&gl_context).unwrap();
            }

            _ => {}
        }
    });
}
