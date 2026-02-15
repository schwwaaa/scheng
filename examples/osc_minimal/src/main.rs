use glow::HasContext;
use scheng_control_osc::OscParamReceiver;
use scheng_runtime_glow::{
    compile_program, create_render_target, EngineError, FullscreenTriangle, FULLSCREEN_VERT,
};
use std::num::NonZeroU32;
use std::time::Instant;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

use glutin::display::GetGlDisplay;
use glutin::prelude::*;

// raw-window-handle 0.5 traits (matches glutin 0.31)
use raw_window_handle::HasRawWindowHandle;

#[derive(Debug, Clone)]
struct AppConfig {
    /// UDP bind address for OSC.
    bind_addr: String,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("[scheng-sdk example] error: {e}");
        std::process::exit(1);
    }
}

fn print_usage_and_exit() -> ! {
    eprintln!(
        "Usage:\n  scheng-example-osc-minimal [--bind 127.0.0.1:9000]\n\nOptions:\n  --bind ADDR     UDP bind address for OSC (default: 127.0.0.1:9000)\n  --help, -h      Show this help and exit\n\nOSC message convention:\n  Send either: /param/<name> <value>   or   /<name> <value>\n\nParameters used by this example:\n  u_gain   (float)  brightness multiplier (default: 1.0)\n  u_speed  (float)  time scale (default: 1.0)\n\nExample sends (TouchOSC / any OSC sender):\n  /param/u_gain 1.25\n  /param/u_speed 0.5\n"
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

fn run() -> Result<(), EngineError> {
    let cfg = parse_args();

    // OSC is optional; if bind fails we error once and exit (no silent failure).
    let mut osc = OscParamReceiver::bind(&cfg.bind_addr)
        .map_err(|e| EngineError::Other(format!("bind OSC {}: {e}", cfg.bind_addr)))?;

    println!("OSC listening on {}", cfg.bind_addr);
    println!("Try sending /param/u_gain 1.25 or /param/u_speed 0.5");

    let event_loop = EventLoop::new();

    let window_builder = WindowBuilder::new()
        .with_title("scheng-sdk: osc minimal")
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

    let fs_tri = unsafe { FullscreenTriangle::new(&gl)? };

    let frag_src = r#"
#version 330 core
in vec2 v_uv;
out vec4 fragColor;
uniform float u_time;
uniform vec2  u_resolution;
uniform float u_gain;
uniform float u_speed;

void main() {
    // Centered UV (-1..1) with correct aspect.
    vec2 p = (v_uv - 0.5) * 2.0;
    p.x *= u_resolution.x / u_resolution.y;

    float t = u_time * u_speed;

    float r = length(p);
    float a = atan(p.y, p.x);

    // Simple animated polar gradient.
    float bands = 0.5 + 0.5*sin(8.0*r - t*2.0);
    float spin  = 0.5 + 0.5*sin(a*3.0 + t);

    vec3 col = vec3(bands, spin, 1.0 - bands);
    col *= u_gain;

    fragColor = vec4(col, 1.0);
}
"#;

    let program = unsafe { compile_program(&gl, FULLSCREEN_VERT, frag_src)? };
    let mut rt = unsafe { create_render_target(&gl, width as i32, height as i32)? };

    let start = Instant::now();

    // OSC-controlled params.
    let mut u_gain: f32 = 1.0;
    let mut u_speed: f32 = 1.0;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,

                WindowEvent::Resized(physical_size) => {
                    let w = physical_size.width.max(1);
                    let h = physical_size.height.max(1);

                    gl_surface.resize(
                        &gl_context,
                        NonZeroU32::new(w).unwrap(),
                        NonZeroU32::new(h).unwrap(),
                    );

                    unsafe {
                        gl.delete_texture(rt.tex);
                        gl.delete_framebuffer(rt.fbo);
                        rt = create_render_target(&gl, w as i32, h as i32).unwrap();
                    }

                    window.request_redraw();
                }

                _ => {}
            },

            Event::MainEventsCleared => window.request_redraw(),

            Event::RedrawRequested(_) => {
                // Drain OSC updates once per frame.
                for (name, val) in osc.poll() {
                    match name.as_str() {
                        "u_gain" => u_gain = val,
                        "u_speed" => u_speed = val,
                        _ => {}
                    }
                }

                let (w, h) = {
                    let s = window.inner_size();
                    (s.width.max(1) as i32, s.height.max(1) as i32)
                };
                let t = start.elapsed().as_secs_f32();

                unsafe {
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(rt.fbo));
                    gl.viewport(0, 0, w, h);

                    gl.use_program(Some(program));

                    if let Some(loc) = gl.get_uniform_location(program, "u_time") {
                        gl.uniform_1_f32(Some(&loc), t);
                    }
                    if let Some(loc) = gl.get_uniform_location(program, "u_resolution") {
                        gl.uniform_2_f32(Some(&loc), w as f32, h as f32);
                    }
                    if let Some(loc) = gl.get_uniform_location(program, "u_gain") {
                        gl.uniform_1_f32(Some(&loc), u_gain);
                    }
                    if let Some(loc) = gl.get_uniform_location(program, "u_speed") {
                        gl.uniform_1_f32(Some(&loc), u_speed);
                    }

                    fs_tri.draw(&gl);

                    gl.use_program(None);
                    gl.bind_framebuffer(glow::FRAMEBUFFER, None);

                    gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(rt.fbo));
                    gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
                    gl.blit_framebuffer(
                        0,
                        0,
                        w,
                        h,
                        0,
                        0,
                        w,
                        h,
                        glow::COLOR_BUFFER_BIT,
                        glow::NEAREST,
                    );
                    gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
                    gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
                }

                gl_surface.swap_buffers(&gl_context).unwrap();
            }

            _ => {}
        }
    });
}
