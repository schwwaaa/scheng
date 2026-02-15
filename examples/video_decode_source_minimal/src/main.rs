// examples/video_decode_source_minimal/src/main.rs
//
// Step 12: VideoDecodeSource (engine-integrated decoder path)
//
// Graph (v1 invariant):
//   VideoDecodeSource (Source) -> ShaderPass (render) -> PixelsOut (Output)
//
// This example:
//   - Creates a GL window
//   - Builds a small scheng graph using VideoDecodeSource
//   - Reads a video config JSON path from the CLI
//   - Feeds decoded video frames into the graph and presents them.
//   - Step 12.1: adds keyboard transport (play/pause, speed, scrub) driving FrameCtx::time.

use std::num::NonZeroU32;
use std::time::Instant;

use glow::HasContext;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextAttributesBuilder, NotCurrentGlContextSurfaceAccessor, PossiblyCurrentContext};
use glutin::display::GetGlDisplay;
use glutin::prelude::{GlConfig, GlDisplay, GlSurface};
use glutin_winit::DisplayBuilder;
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent};
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

/// Simple playback transport driven by keyboard.
///
/// This is *purely* a front-end for FrameCtx::time — the decoder node stays
/// unchanged and just reads whatever timeline we feed it.
#[derive(Debug, Clone, Copy)]
struct Transport {
    /// Current playhead position in seconds.
    playhead: f32,
    /// Playback speed multiplier (1.0 = realtime).
    speed: f32,
    /// Whether playback is paused.
    paused: bool,
    /// Last time we updated the playhead.
    last_instant: Instant,
}

impl Transport {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            playhead: 0.0,
            speed: 1.0,
            paused: false,
            last_instant: now,
        }
    }

    /// Advance the internal clock based on wall time.
    fn update(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_instant).as_secs_f32();
        self.last_instant = now;

        if !self.paused {
            self.playhead = (self.playhead + dt * self.speed).max(0.0);
        }
    }

    /// Handle a single keyboard event.
    ///
    /// Keys (for now, hard-coded; JSON/OSC can layer on top later):
    ///   Space  -> toggle play/pause
    ///   Left   -> scrub -0.5s
    ///   Right  -> scrub +0.5s
    ///   Down   -> half speed
    ///   Up     -> double speed
    ///   Home   -> jump to t = 0
    fn handle_key(&mut self, input: &KeyboardInput) {
        if input.state != ElementState::Pressed {
            return;
        }

        if let Some(key) = input.virtual_keycode {
            match key {
                VirtualKeyCode::Space => {
                    self.paused = !self.paused;
                    println!(
                        "transport: {} (speed {:.2}x, t = {:.2}s)",
                        if self.paused { "paused" } else { "playing" },
                        self.speed,
                        self.playhead
                    );
                }
                VirtualKeyCode::Left => {
                    self.playhead = (self.playhead - 0.5).max(0.0);
                    println!("transport: scrub back to {:.2}s", self.playhead);
                }
                VirtualKeyCode::Right => {
                    self.playhead += 0.5;
                    println!("transport: scrub fwd to {:.2}s", self.playhead);
                }
                VirtualKeyCode::Down => {
                    self.speed *= 0.5;
                    println!("transport: speed {:.2}x", self.speed);
                }
                VirtualKeyCode::Up => {
                    self.speed *= 2.0;
                    println!("transport: speed {:.2}x", self.speed);
                }
                VirtualKeyCode::Home => {
                    self.playhead = 0.0;
                    println!("transport: reset to start");
                }
                _ => {}
            }
        }
    }
}

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
            .with_title("scheng: video_decode_source_minimal")
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
    let not_current_gl_context =
        unsafe { gl_display.create_context(&gl_config, &context_attributes).unwrap() };

    let size = window.inner_size();
    let attrs =
        glutin::surface::SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new().build(
            raw_window_handle,
            NonZeroU32::new(size.width.max(1)).unwrap(),
            NonZeroU32::new(size.height.max(1)).unwrap(),
        );

    let gl_surface = unsafe { gl_display.create_window_surface(&gl_config, &attrs).unwrap() };

    let gl_context = not_current_gl_context
        .make_current(&gl_surface)
        .unwrap();

    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            gl_display.get_proc_address(
                std::ffi::CStr::from_bytes_with_nul_unchecked(
                    format!("{s}\0").as_bytes(),
                ),
            ) as *const _
        })
    };

    (window, gl_surface, gl_context, gl)
}

fn main() {
    let event_loop = winit::event_loop::EventLoop::new();
    let (window, gl_surface, gl_context, gl) = make_gl(&event_loop);

    // Graph: VideoDecodeSource -> ShaderPass -> PixelsOut
    let mut g = graph::Graph::new();

    let video_src = g.add_node(graph::NodeKind::VideoDecodeSource);
    let pass = g.add_node(graph::NodeKind::ShaderPass);
    let out = g.add_node(graph::NodeKind::PixelsOut);

    // Use named ports; PortId values are global and not stable/obvious.
    g.connect_named(video_src, "out", pass, "in")
        .expect("connect video->pass");
    g.connect_named(pass, "out", out, "in")
        .expect("connect pass->out");

    let plan = g.compile().expect("compile plan");

    let mut props = rt::NodeProps::default();
    props.output_names.insert(out, "preview".into());

    props.shader_sources.insert(
        pass,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: PRESENT_FRAG.to_string(),
            origin: Some("video_decode_source_minimal:passthrough".into()),
        },
    );

    // Video decode config (JSON) comes from CLI: first arg is path.
    let cfg_path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: scheng-example-video-decode-source-minimal <video_config.json>");
        std::process::exit(2);
    });
    props
        .video_decode_json
        .insert(video_src, std::path::PathBuf::from(cfg_path));

    let mut state = unsafe { rt::RuntimeState::new(&gl).expect("rt state") };
    let presenter = unsafe { Presenter::new(&gl).expect("presenter") };

    // Legacy time origin placeholder (kept to avoid ripping out code; we now
    // drive FrameCtx::time from `transport` instead).
    let _t0 = Instant::now();

    // Step 12.1: keyboard-only transport driving FrameCtx::time.
    let mut transport = Transport::new();

    println!("--- keyboard transport ---");
    println!("Space : play / pause");
    println!("Left  : scrub -0.5s");
    println!("Right : scrub +0.5s");
    println!("Down  : half speed");
    println!("Up    : double speed");
    println!("Home  : jump to start");
    println!("--------------------------");

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
                WindowEvent::KeyboardInput { input, .. } => {
                    // Feed keyboard into our transport.
                    transport.handle_key(&input);
                }
                _ => {}
            },
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                // Advance transport and use its playhead as FrameCtx::time.
                transport.update();
                let playhead = transport.playhead;

                let size = window.inner_size();
                let w = size.width as i32;
                let h = size.height as i32;

                let frame = rt::FrameCtx {
                    width: w,
                    height: h,
                    time: playhead,
                    frame: 0,
                };

                unsafe {
                    let outs = rt::execute_plan_outputs(
                        &gl,
                        &g,
                        &plan,
                        &mut state,
                        &props,
                        frame,
                    )
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
