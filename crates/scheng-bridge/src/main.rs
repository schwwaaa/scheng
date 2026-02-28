mod protocol;
mod registry;
mod state;
mod ws;
mod osc;
mod midi;

use glow::HasContext;
use raw_window_handle::HasRawWindowHandle;
use scheng_graph::{Graph, NodeKind};
use scheng_runtime_glow::{
    execute_plan_to_sink, EngineError, ExecOutput, FrameCtx, NodeProps, OutputSink,
    RuntimeState, ShaderSource, FULLSCREEN_VERT,
};
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use ws::{RenderBundle, SharedBundle};

struct PresentSink { w: i32, h: i32 }
impl OutputSink for PresentSink {
    fn consume(&mut self, gl: &glow::Context, out: &ExecOutput) {
        unsafe {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(out.fbo));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
            gl.blit_framebuffer(0, 0, out.width, out.height, 0, 0, self.w, self.h,
                glow::COLOR_BUFFER_BIT, glow::LINEAR);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
        }
    }
}

fn startup_bundle() -> Result<RenderBundle, EngineError> {
    // Exactly graph_minimal — proven working
    let mut graph = Graph::new();
    let src  = graph.add_node(NodeKind::ShaderSource);
    let pass = graph.add_node(NodeKind::ShaderPass);
    let out  = graph.add_node(NodeKind::PixelsOut);
    graph.connect_named(src, "out", pass, "in")?;
    graph.connect_named(pass, "out", out, "in")?;
    let plan = graph.compile()?;
    let mut props = NodeProps::default();
    props.shader_sources.insert(src, ShaderSource {
        vert: FULLSCREEN_VERT.to_string(),
        frag: r#"#version 330 core
in vec2 v_uv;
out vec4 fragColor;
uniform float u_time;
void main() {
    vec3 col = 0.5 + 0.5 * cos(u_time + v_uv.xyx + vec3(0.0, 2.1, 4.2));
    fragColor = vec4(col, 1.0);
}
"#.to_string(),
        origin: Some("startup".into()),
    });
    // Startup bundle has no bridge nodes — empty id_map is correct.
    // Gets replaced on first Compile from the editor.
    let id_map = std::collections::HashMap::new();
    Ok(RenderBundle { id_map, graph, plan, props })
}

fn main() {
    if let Err(e) = run() {
        eprintln!("[scheng-bridge] fatal: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), EngineError> {
    // Start with the proven startup bundle. Compile from browser replaces it.
    let bundle: SharedBundle = Arc::new(Mutex::new(Some(startup_bundle()?)));

    {
        let ws_state = Arc::new(Mutex::new(state::BridgeState::new()));
        let b = bundle.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all().build().unwrap();
            let addr: std::net::SocketAddr = std::env::var("SCHENG_BRIDGE_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:7777".into())
                .parse().unwrap();
            eprintln!("[bridge] ws on {addr}");
            rt.block_on(async move {
                let (tx, _) = tokio::sync::broadcast::channel::<String>(256);

                // OSC — runs in a blocking thread (UDP recv loop)
                {
                    let ws2 = ws_state.clone();
                    let b2  = b.clone();
                    let tx2 = tx.clone();
                    std::thread::spawn(move || osc::run_osc(ws2, b2, tx2));
                }

                // MIDI — runs in a blocking thread (midir callback thread)
                {
                    let ws3 = ws_state.clone();
                    let b3  = b.clone();
                    let tx3 = tx.clone();
                    std::thread::spawn(move || midi::run_midi(ws3, b3, tx3));
                }

                ws::run_ws_server(addr, ws_state, b, tx).await;
            });
        });
    }

    let event_loop = EventLoop::new();
    let (window, gl_config) = glutin_winit::DisplayBuilder::new()
        .with_window_builder(Some(WindowBuilder::new()
            .with_title("scheng-bridge")
            .with_inner_size(winit::dpi::LogicalSize::new(960.0_f64, 540.0_f64))))
        .build(&event_loop,
            glutin::config::ConfigTemplateBuilder::new()
                .with_alpha_size(8).with_depth_size(0).with_stencil_size(0).with_transparency(false),
            |configs| configs.reduce(|a, b| if b.num_samples() > a.num_samples() { b } else { a }).unwrap())
        .map_err(|e| EngineError::GlCreate(format!("{e}")))?;

    let window     = window.ok_or_else(|| EngineError::GlCreate("no window".into()))?;
    let gl_display = gl_config.display();
    let raw_handle = window.raw_window_handle();

    let not_current = unsafe {
        let a = glutin::context::ContextAttributesBuilder::new()
            .with_profile(glutin::context::GlProfile::Core).build(Some(raw_handle));
        let b = glutin::context::ContextAttributesBuilder::new()
            .with_profile(glutin::context::GlProfile::Core).build(None);
        gl_display.create_context(&gl_config, &a)
            .or_else(|_| gl_display.create_context(&gl_config, &b))
            .map_err(|e| EngineError::GlCreate(format!("{e}")))?
    };

    let (w0, h0) = { let s = window.inner_size(); (s.width.max(1), s.height.max(1)) };
    let gl_surface = unsafe {
        gl_display.create_window_surface(&gl_config,
            &glutin::surface::SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new()
                .build(raw_handle, NonZeroU32::new(w0).unwrap(), NonZeroU32::new(h0).unwrap()))
            .map_err(|e| EngineError::GlCreate(format!("{e}")))?
    };
    let gl_context = not_current.make_current(&gl_surface)
        .map_err(|e| EngineError::GlCreate(format!("{e}")))?;
    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            gl_display.get_proc_address(std::ffi::CString::new(s).unwrap().as_c_str()) as *const _
        })
    };

    let start = Instant::now();
    let mut rt_state = unsafe { RuntimeState::new(&gl)? };
    let mut frame_num: u64 = 0;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => { unsafe { rt_state.destroy(&gl) }; *control_flow = ControlFlow::Exit; }
                WindowEvent::Resized(s) => { gl_surface.resize(&gl_context,
                    NonZeroU32::new(s.width.max(1)).unwrap(), NonZeroU32::new(s.height.max(1)).unwrap()); }
                _ => {}
            },
            Event::MainEventsCleared => window.request_redraw(),
            Event::RedrawRequested(_) => {
                let (w, h) = { let s = window.inner_size(); (s.width.max(1) as i32, s.height.max(1) as i32) };
                frame_num += 1;
                let frame = FrameCtx { time: start.elapsed().as_secs_f32(), width: w, height: h, frame: frame_num };

                let b = bundle.lock().unwrap();
                if let Some(ref b) = *b {
                    let mut sink = PresentSink { w, h };
                    if let Err(e) = unsafe {
                        execute_plan_to_sink(&gl, &b.graph, &b.plan, &mut rt_state, &b.props, frame, &mut sink)
                    } {
                        eprintln!("[frame {frame_num}] ERROR: {e:?}");
                    }
                } else {
                    unsafe {
                        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                        gl.viewport(0, 0, w, h);
                        gl.clear_color(0.08, 0.04, 0.14, 1.0);
                        gl.clear(glow::COLOR_BUFFER_BIT);
                    }
                }
                drop(b);
                gl_surface.swap_buffers(&gl_context).unwrap();
            }
            _ => {}
        }
    });
}
