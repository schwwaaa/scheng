use glow::HasContext;
use scheng_passes::PingPongTarget;
use scheng_runtime_glow::{compile_program, EngineError, FullscreenTriangle, FULLSCREEN_VERT};

use std::num::NonZeroU32;
use std::time::Instant;

use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use raw_window_handle::HasRawWindowHandle;

fn main() {
    if let Err(e) = run() {
        eprintln!("[feedback_pingpong] error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), EngineError> {
    let event_loop = EventLoop::new();

    let window_builder = WindowBuilder::new()
        .with_title("scheng-sdk: feedback ping-pong")
        .with_inner_size(winit::dpi::LogicalSize::new(960.0, 540.0));

    let template = glutin::config::ConfigTemplateBuilder::new().with_alpha_size(8);

    let display_builder =
        glutin_winit::DisplayBuilder::new().with_window_builder(Some(window_builder));

    let (window, gl_config) = display_builder
        .build(&event_loop, template, |mut configs| configs.next().unwrap())
        .map_err(|e| EngineError::GlCreate(format!("DisplayBuilder.build: {e}")))?;

    let window = window
        .ok_or_else(|| EngineError::GlCreate("DisplayBuilder did not create a window".into()))?;
    let gl_display = gl_config.display();

    let raw_window_handle = window.raw_window_handle();

    let context_attributes = glutin::context::ContextAttributesBuilder::new()
        .with_profile(glutin::context::GlProfile::Core)
        .build(Some(raw_window_handle));

    let not_current_gl_context = unsafe {
        gl_display
            .create_context(&gl_config, &context_attributes)
            .map_err(|e| EngineError::GlCreate(format!("create_context: {e}")))?
    };

    let size = window.inner_size();
    let attrs = glutin::surface::SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new()
        .build(
            raw_window_handle,
            NonZeroU32::new(size.width.max(1)).unwrap(),
            NonZeroU32::new(size.height.max(1)).unwrap(),
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

    // Feedback shader: sample previous frame, nudge UV, decay slightly, add a moving injection.
    let frag_src = r#"
#version 330 core
in vec2 v_uv;
out vec4 fragColor;

uniform sampler2D u_feedback;
uniform vec2  u_resolution;
uniform float u_time;

// OSC-controlled (host can override defaults)
uniform float u_decay;   // e.g. 0.9985
uniform float u_gain;    // e.g. 1.02
uniform float u_inject;  // e.g. 1.0
uniform float u_spin;    // radians/sec

void main() {
    vec2 uv = clamp(v_uv * 0.5, 0.0, 1.0);

    // Rotate-around-center feedback sample (spin)
    vec2 p = uv - 0.5;
    float a = u_spin * u_time;
    mat2 R = mat2(cos(a), -sin(a), sin(a), cos(a));
    vec2 pr = R * p;
    vec2 uv_spin = pr + 0.5;

    // Small offset to keep motion alive
    vec2 off = vec2(sin(u_time*0.9), cos(u_time*1.1)) * (2.0 / max(u_resolution, vec2(1.0)));
    vec4 prev = texture(u_feedback, clamp(uv_spin + off, 0.0, 1.0));

    // Feedback shaping
    prev *= u_decay;
    prev *= u_gain;

    // Inject a moving dot (bigger + brighter so you see trails)
    vec2 c = vec2(0.5 + 0.25*sin(u_time*0.7), 0.5 + 0.25*cos(u_time*0.8));
    float d = length(uv - c);
    float dot = smoothstep(0.08, 0.0, d) * u_inject;
    vec4 inj = vec4(dot, dot*0.8, dot, 1.0);

    fragColor = max(prev, inj);
}
"#;

    let program = unsafe { compile_program(&gl, FULLSCREEN_VERT, frag_src)? };

    let mut ping = unsafe { PingPongTarget::new(&gl, size.width as i32, size.height as i32)? };

    // --- OSC (host/policy) ---
    // Send messages like:
    //   /param/u_decay 0.9985
    //   /param/u_gain  1.02
    //   /param/u_inject 1.0
    //   /param/u_spin  0.6
    //
    // Default bind: 127.0.0.1:9000
    let mut osc = scheng_control_osc::OscParamReceiver::bind("127.0.0.1:9000")
        .map_err(|e| EngineError::GlCreate(format!("OSC bind failed: {e}")))?;

    // Tunables (defaults)
    let mut u_decay: f32 = 0.9985;
    let mut u_gain: f32 = 1.02;
    let mut u_inject: f32 = 1.0;
    let mut u_spin: f32 = 0.35;

    let start = Instant::now();

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
                        ping.resize(&gl, w as i32, h as i32).unwrap();
                    }

                    window.request_redraw();
                }

                _ => {}
            },

            Event::MainEventsCleared => window.request_redraw(),

            Event::RedrawRequested(_) => {
                let s = window.inner_size();
                let w = s.width.max(1) as i32;
                let h = s.height.max(1) as i32;
                let t = start.elapsed().as_secs_f32();

                // Apply OSC param updates (non-blocking)
                for (name, val) in osc.poll() {
                    match name.as_str() {
                        "u_decay" => u_decay = val,
                        "u_gain" => u_gain = val,
                        "u_inject" => u_inject = val,
                        "u_spin" => u_spin = val,
                        _ => {}
                    }
                }

                unsafe {
                    // Render into NEXT target using PREV as feedback input
                    let next = ping.next_target();

                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(next.fbo));
                    gl.viewport(0, 0, w, h);

                    gl.use_program(Some(program));

                    // uniforms
                    if let Some(loc) = gl.get_uniform_location(program, "u_time") {
                        gl.uniform_1_f32(Some(&loc), t);
                    }
                    if let Some(loc) = gl.get_uniform_location(program, "u_resolution") {
                        gl.uniform_2_f32(Some(&loc), w as f32, h as f32);
                    }
                    if let Some(loc) = gl.get_uniform_location(program, "u_decay") {
                        gl.uniform_1_f32(Some(&loc), u_decay);
                    }
                    if let Some(loc) = gl.get_uniform_location(program, "u_gain") {
                        gl.uniform_1_f32(Some(&loc), u_gain);
                    }
                    if let Some(loc) = gl.get_uniform_location(program, "u_inject") {
                        gl.uniform_1_f32(Some(&loc), u_inject);
                    }
                    if let Some(loc) = gl.get_uniform_location(program, "u_spin") {
                        gl.uniform_1_f32(Some(&loc), u_spin);
                    }

                    // bind feedback texture to unit 0
                    gl.active_texture(glow::TEXTURE0);
                    gl.bind_texture(glow::TEXTURE_2D, Some(ping.prev_tex()));
                    if let Some(loc) = gl.get_uniform_location(program, "u_feedback") {
                        gl.uniform_1_i32(Some(&loc), 0);
                    }

                    fs_tri.draw(&gl);

                    gl.bind_texture(glow::TEXTURE_2D, None);
                    gl.use_program(None);
                    gl.bind_framebuffer(glow::FRAMEBUFFER, None);

                    // Blit NEXT to window
                    gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(next.fbo));
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

                // commit frame
                ping.swap();
            }

            _ => {}
        }
    });
}
