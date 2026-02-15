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

fn shader_bg() -> ShaderSource {
    ShaderSource {
        origin: Some("mixer2/bg".to_string()),
        vert: FULLSCREEN_VERT.to_string(),
        frag: r#"#version 330 core
in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform vec2  u_resolution;

void main() {
    vec2 uv = v_uv;
    float t = u_time * 0.7;
    float a = 0.5 + 0.5*sin((uv.x*6.0 + t) * 3.14159);
    float b = 0.5 + 0.5*sin((uv.y*4.0 - t) * 3.14159);
    vec3 col = vec3(a, b, 0.35 + 0.65*sin(t));
    fragColor = vec4(col, 1.0);
}
"#
        .to_string(),
    }
}

fn shader_fg() -> ShaderSource {
    ShaderSource {
        origin: Some("mixer2/fg".to_string()),
        vert: FULLSCREEN_VERT.to_string(),
        frag: r#"#version 330 core
in vec2 v_uv;
out vec4 fragColor;

uniform float u_time;
uniform vec2  u_resolution;

void main() {
    vec2 uv = v_uv;
    // animated soft circle mask
    vec2 p = uv * 2.0 - 1.0;
    float t = u_time * 0.9;
    p.x += 0.2 * sin(t);
    p.y += 0.15 * cos(t * 0.7);
    float d = length(p);
    float alpha = smoothstep(0.55, 0.35, d);
    vec3 col = vec3(0.95, 0.25, 0.6);
    fragColor = vec4(col, alpha);
}
"#
        .to_string(),
    }
}

fn shader_mix() -> ShaderSource {
    ShaderSource {
        origin: Some("mixer2/mix".to_string()),
        vert: FULLSCREEN_VERT.to_string(),
        frag: r#"#version 330 core
in vec2 v_uv;
out vec4 fragColor;

uniform sampler2D iChannel0;
uniform sampler2D iChannel1;
uniform float u_time;
uniform vec2  u_resolution;

void main() {
    vec2 uv = v_uv;
    vec3 bg = texture(iChannel0, uv).rgb;
    vec4 fg = texture(iChannel1, uv);
    // basic alpha composite
    vec3 col = mix(bg, fg.rgb, fg.a);
    fragColor = vec4(col, 1.0);
}
"#
        .to_string(),
    }
}

fn main() -> Result<(), EngineError> {
    // Graph (C3d validation):
    //   ShaderSource(bg) -> ShaderPass(bg) ----\
    //                                         Mixer(Crossfade) -> PixelsOut
    //   ShaderSource(fg) -> ShaderPass(fg) ----/
    //
    // Runtime binding order is semantic (Option A):
    //   Mixer input "a" -> iChannel0
    //   Mixer input "b" -> iChannel1

    let mut graph = Graph::new();
    let src_bg = graph.add_node(NodeKind::ShaderSource);
    let pass_bg = graph.add_node(NodeKind::ShaderPass);
    let src_fg = graph.add_node(NodeKind::ShaderSource);
    let pass_fg = graph.add_node(NodeKind::ShaderPass);
    let mix = graph.add_node(NodeKind::Crossfade);
    let out = graph.add_node(NodeKind::PixelsOut);

    // Ports are convention-based in v0: Source(out), Processor(in/out), Mixer(a/b/out), Output(in).
    let src_bg_out = graph.find_port(src_bg, "out", PortDir::Out).unwrap();
    let pass_bg_in = graph.find_port(pass_bg, "in", PortDir::In).unwrap();
    let pass_bg_out = graph.find_port(pass_bg, "out", PortDir::Out).unwrap();

    let src_fg_out = graph.find_port(src_fg, "out", PortDir::Out).unwrap();
    let pass_fg_in = graph.find_port(pass_fg, "in", PortDir::In).unwrap();
    let pass_fg_out = graph.find_port(pass_fg, "out", PortDir::Out).unwrap();

    let mix_a = graph.find_port(mix, "a", PortDir::In).unwrap();
    let mix_b = graph.find_port(mix, "b", PortDir::In).unwrap();
    let mix_out = graph.find_port(mix, "out", PortDir::Out).unwrap();

    let out_in = graph.find_port(out, "in", PortDir::In).unwrap();

    graph.connect(
        scheng_graph::Endpoint {
            node: src_bg,
            port: src_bg_out,
            dir: PortDir::Out,
        },
        scheng_graph::Endpoint {
            node: pass_bg,
            port: pass_bg_in,
            dir: PortDir::In,
        },
    )?;
    graph.connect(
        scheng_graph::Endpoint {
            node: src_fg,
            port: src_fg_out,
            dir: PortDir::Out,
        },
        scheng_graph::Endpoint {
            node: pass_fg,
            port: pass_fg_in,
            dir: PortDir::In,
        },
    )?;

    graph.connect(
        scheng_graph::Endpoint {
            node: pass_bg,
            port: pass_bg_out,
            dir: PortDir::Out,
        },
        scheng_graph::Endpoint {
            node: mix,
            port: mix_a,
            dir: PortDir::In,
        },
    )?;
    graph.connect(
        scheng_graph::Endpoint {
            node: pass_fg,
            port: pass_fg_out,
            dir: PortDir::Out,
        },
        scheng_graph::Endpoint {
            node: mix,
            port: mix_b,
            dir: PortDir::In,
        },
    )?;
    graph.connect(
        scheng_graph::Endpoint {
            node: mix,
            port: mix_out,
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
    // ShaderPass nodes resolve via incoming ShaderSource edges (back-compat).
    props.shader_sources.insert(src_bg, shader_bg());
    props.shader_sources.insert(src_fg, shader_fg());
    // Mixer node must be provided explicitly.
    props.shader_sources.insert(mix, shader_mix());

    // --- Window / GL context ---
    let event_loop = EventLoop::new();
    let window_builder = WindowBuilder::new()
        .with_title("scheng graph_mixer2 (C3d)")
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
                    time: start.elapsed().as_secs_f32(),
                    width: size.width as i32,
                    height: size.height as i32,
                    frame: 0,
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
