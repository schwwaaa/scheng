use glow::HasContext;
use scheng_runtime_glow::{
    compile_program, create_render_target, EngineError, FullscreenTriangle, FULLSCREEN_VERT,
};
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use raw_window_handle::HasRawWindowHandle;

fn main() {
    if let Err(e) = run() {
        eprintln!("[render_target_only] error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), EngineError> {
    let event_loop = EventLoop::new();

    let window_builder = WindowBuilder::new()
        .with_title("scheng-sdk: render_target_only (prints checksum then exits)")
        .with_inner_size(winit::dpi::LogicalSize::new(256.0, 256.0));

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

    // Build runtime objects
    let fs_tri = unsafe { FullscreenTriangle::new(&gl)? };

    // Solid color shader (easy to checksum)
    let frag_src = r#"
#version 330 core
in vec2 v_uv;
out vec4 fragColor;
void main() {
    fragColor = vec4(0.1, 0.2, 0.3, 1.0);
}
"#;
    let program = unsafe { compile_program(&gl, FULLSCREEN_VERT, frag_src)? };

    // Render to a small target and read back a 4x4 block from the corner.
    let w: i32 = 64;
    let h: i32 = 64;
    let rt = unsafe { create_render_target(&gl, w, h)? };

    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(rt.fbo));
        gl.viewport(0, 0, w, h);
        gl.use_program(Some(program));
        fs_tri.draw(&gl);
        gl.use_program(None);

        // Read back 4x4 RGBA8 pixels
        let mut px = vec![0u8; 4 * 4 * 4];
        gl.read_pixels(
            0,
            0,
            4,
            4,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(&mut px),
        );

        // Compute a simple checksum
        let mut sum: u64 = 0;
        for b in &px {
            sum = sum.wrapping_add(*b as u64);
        }
        println!("[render_target_only] checksum(sum of 4x4 RGBA bytes) = {sum}");

        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
    }

    // Exit immediately (no loop); window may flash briefly depending on platform.
    drop(gl_surface);
    drop(gl_context);
    Ok(())
}
