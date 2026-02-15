use glow::HasContext;
use scheng_core::EngineError;
use scheng_graph::{Graph, NodeKind, PortDir};
use scheng_runtime_glow::{
    execute_plan_to_sink, FrameCtx, NodeProps, OutputSink, RuntimeState, ShaderSource,
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

use raw_window_handle::HasRawWindowHandle;
use std::time::Instant;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

use glutin::config::ConfigTemplateBuilder;
use glutin::config::GlConfig;
use glutin::context::{
    ContextApi, ContextAttributesBuilder, NotCurrentGlContextSurfaceAccessor, Version,
};
use glutin::display::GetGlDisplay;
use glutin::display::GlDisplay;
use glutin::prelude::GlSurface;
use glutin::surface::{SurfaceAttributesBuilder, SwapInterval, WindowSurface};

const FULLSCREEN_VERT: &str = r#"#version 330 core
layout(location=0) in vec2 a_pos;
out vec2 v_uv;
void main() {
    // a_pos is a fullscreen triangle in clip space.
    gl_Position = vec4(a_pos, 0.0, 1.0);
    // Map clip-space -> [0..1] UV
    v_uv = a_pos * 0.5 + 0.5;
}
"#;

fn shader_a() -> ShaderSource {
    ShaderSource {
        origin: Some("chain2/pass_a".to_string()),
        vert: FULLSCREEN_VERT.to_string(),
        frag: r#"#version 330 core
in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform vec2  u_resolution;

void main() {
    vec2 uv = v_uv;
    // animated gradient bands
    float t = u_time * 0.7;
    float a = 0.5 + 0.5*sin((uv.x*6.0 + t) * 3.14159);
    float b = 0.5 + 0.5*sin((uv.y*4.0 - t) * 3.14159);
    vec3 col = vec3(a, b, 0.5 + 0.5*sin(t));
    fragColor = vec4(col, 1.0);
}
"#
        .to_string(),
    }
}

fn shader_b() -> ShaderSource {
    ShaderSource {
        origin: Some("chain2/pass_b".to_string()),
        vert: FULLSCREEN_VERT.to_string(),
        frag: r#"#version 330 core
in vec2 v_uv;
out vec4 fragColor;

uniform sampler2D iChannel0;
uniform float u_time;
uniform vec2  u_resolution;

void main() {
    vec2 uv = v_uv;
    // subtle warp that depends on time
    float t = u_time * 0.8;
    uv.x += 0.02 * sin(uv.y * 20.0 + t);
    uv.y += 0.02 * cos(uv.x * 18.0 - t);

    vec3 src = texture(iChannel0, uv).rgb;
    // post effect: contrast + inverted tint
    src = pow(src, vec3(1.2));
    vec3 col = vec3(1.0) - src;
    fragColor = vec4(col, 1.0);
}
"#
        .to_string(),
    }
}

fn main() -> Result<(), EngineError> {
    // Graph:
    //   ShaderSource(A) -> ShaderPass(A) -> ShaderPass(B) -> PixelsOut
    // ShaderPass(A) resolves its code via incoming ShaderSource edge (back-compat).
    // ShaderPass(B) resolves its code via NodeProps keyed by pass node id.
    // Texture routing uses the ShaderPass(A) -> ShaderPass(B) edge.

    let mut graph = Graph::new();
    let src_a = graph.add_node(NodeKind::ShaderSource);
    let pass_a = graph.add_node(NodeKind::ShaderPass);
    let pass_b = graph.add_node(NodeKind::ShaderPass);
    let out = graph.add_node(NodeKind::PixelsOut);

    // Ports are convention-based in v0: Source out, Processor in/out, Output in.
    let src_a_out = graph
        .node(src_a)
        .unwrap()
        .ports
        .iter()
        .find(|p| p.dir == PortDir::Out)
        .unwrap()
        .id;
    let pass_a_in = graph
        .node(pass_a)
        .unwrap()
        .ports
        .iter()
        .find(|p| p.dir == PortDir::In)
        .unwrap()
        .id;
    let pass_a_out = graph
        .node(pass_a)
        .unwrap()
        .ports
        .iter()
        .find(|p| p.dir == PortDir::Out)
        .unwrap()
        .id;

    let pass_b_in = graph
        .node(pass_b)
        .unwrap()
        .ports
        .iter()
        .find(|p| p.dir == PortDir::In)
        .unwrap()
        .id;
    let pass_b_out = graph
        .node(pass_b)
        .unwrap()
        .ports
        .iter()
        .find(|p| p.dir == PortDir::Out)
        .unwrap()
        .id;

    let out_in = graph
        .node(out)
        .unwrap()
        .ports
        .iter()
        .find(|p| p.dir == PortDir::In)
        .unwrap()
        .id;

    graph.connect(
        scheng_graph::Endpoint {
            node: src_a,
            port: src_a_out,
            dir: PortDir::Out,
        },
        scheng_graph::Endpoint {
            node: pass_a,
            port: pass_a_in,
            dir: PortDir::In,
        },
    )?;
    graph.connect(
        scheng_graph::Endpoint {
            node: pass_a,
            port: pass_a_out,
            dir: PortDir::Out,
        },
        scheng_graph::Endpoint {
            node: pass_b,
            port: pass_b_in,
            dir: PortDir::In,
        },
    )?;
    graph.connect(
        scheng_graph::Endpoint {
            node: pass_b,
            port: pass_b_out,
            dir: PortDir::Out,
        },
        scheng_graph::Endpoint {
            node: out,
            port: out_in,
            dir: PortDir::In,
        },
    )?;

    let plan = graph.compile()?;

    let mut props = NodeProps::default();
    props.shader_sources.insert(src_a, shader_a());
    props.shader_sources.insert(pass_b, shader_b());

    // --- Window / GL context ---
    let event_loop = EventLoop::new();
    let window_builder = WindowBuilder::new()
        .with_title("scheng graph_chain2 (C3c)")
        .with_inner_size(winit::dpi::LogicalSize::new(960.0, 540.0));

    let template = ConfigTemplateBuilder::new()
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

    let window = window.unwrap();

    let raw_window_handle = window.raw_window_handle();

    let gl_display = gl_config.display();
    let context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .build(Some(raw_window_handle));

    let not_current = unsafe { gl_display.create_context(&gl_config, &context_attributes) }
        .map_err(|e| EngineError::GlCreate(format!("create_context: {e}")))?;

    let (width, height): (u32, u32) = window.inner_size().into();

    let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        raw_window_handle,
        std::num::NonZeroU32::new(width.max(1)).unwrap(),
        std::num::NonZeroU32::new(height.max(1)).unwrap(),
    );

    let gl_surface = unsafe { gl_display.create_window_surface(&gl_config, &attrs) }
        .map_err(|e| EngineError::GlCreate(format!("create_window_surface: {e}")))?;

    let gl_context = not_current
        .make_current(&gl_surface)
        .map_err(|e| EngineError::GlCreate(format!("make_current: {e}")))?;

    gl_surface
        .set_swap_interval(
            &gl_context,
            SwapInterval::Wait(std::num::NonZeroU32::new(1).unwrap()),
        )
        .ok();

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
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                WindowEvent::Resized(size) => {
                    let w = size.width.max(1);
                    let h = size.height.max(1);
                    gl_surface.resize(
                        &gl_context,
                        std::num::NonZeroU32::new(w).unwrap(),
                        std::num::NonZeroU32::new(h).unwrap(),
                    );
                }
                _ => {}
            },

            Event::RedrawRequested(_) => unsafe {
                let size = window.inner_size();
                let frame = FrameCtx {
                    frame: 0,
                    time: start.elapsed().as_secs_f32(),
                    width: size.width as i32,
                    height: size.height as i32,
                };

                let mut sink = PresentBlitSink {
                    w: size.width as i32,
                    h: size.height as i32,
                };
                match execute_plan_to_sink(&gl, &graph, &plan, &mut state, &props, frame, &mut sink)
                {
                    Ok(out_tex) => {
                        // Blit from the plan output framebuffer into the window framebuffer.
                        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(out_tex.fbo));
                        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
                        gl.blit_framebuffer(
                            0,
                            0,
                            out_tex.width,
                            out_tex.height,
                            0,
                            0,
                            out_tex.width,
                            out_tex.height,
                            glow::COLOR_BUFFER_BIT,
                            glow::NEAREST,
                        );
                        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
                        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
                    }
                    Err(e) => eprintln!("execute_plan error: {e}"),
                }

                gl_surface.swap_buffers(&gl_context).unwrap();
            },

            Event::MainEventsCleared => {
                window.request_redraw();
            }

            _ => {}
        }
    });
}
