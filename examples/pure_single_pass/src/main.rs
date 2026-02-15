use glow::HasContext;
use scheng_runtime_glow::{compile_program, EngineError, FullscreenTriangle, FULLSCREEN_VERT};
use std::time::Instant;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use raw_window_handle::HasRawWindowHandle;

fn main() {
    if let Err(e) = run() {
        eprintln!("[pure_single_pass] error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), EngineError> {
    let event_loop = EventLoop::new();

    let window_builder = WindowBuilder::new()
        .with_title("scheng-sdk: pure single pass")
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
            size.width.try_into().unwrap(),
            size.height.try_into().unwrap(),
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
void main() {
    float t = 0.5 + 0.5*sin(u_time);
    vec2 uv01 = clamp(v_uv * 0.5, 0.0, 1.0);
    fragColor = vec4(uv01.x, uv01.y, t, 1.0);
}
"#;

    let program = unsafe { compile_program(&gl, FULLSCREEN_VERT, frag_src)? };
    let start = Instant::now();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,

            Event::MainEventsCleared => window.request_redraw(),
            Event::RedrawRequested(_) => unsafe {
                let t = start.elapsed().as_secs_f32();
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                let s = window.inner_size();
                gl.viewport(0, 0, s.width as i32, s.height as i32);

                gl.use_program(Some(program));
                if let Some(loc) = gl.get_uniform_location(program, "u_time") {
                    gl.uniform_1_f32(Some(&loc), t);
                }
                fs_tri.draw(&gl);
                gl.use_program(None);

                gl_surface.swap_buffers(&gl_context).unwrap();
            },
            _ => {}
        }
    });
}
