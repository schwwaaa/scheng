use glow::HasContext;
use scheng_control_osc::OscParamReceiver;
use scheng_passes::PingPongTarget;
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
use raw_window_handle::HasRawWindowHandle;

fn main() {
    if let Err(e) = run() {
        eprintln!("[feedback_orb] error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), EngineError> {
    let event_loop = EventLoop::new();

    let window_builder = WindowBuilder::new()
        .with_title("scheng-sdk: orb source + dual-input feedback")
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

    // --- OSC ---
    // /param/u_decay   0.9985
    // /param/u_gain    1.02
    // /param/u_mix     0.8
    // /param/u_spin    0.35
    // /param/u_zoom    1.0
    // /param/u_blur    0.0
    // /param/u_orb_size 0.22
    // /param/u_orb_x   0.5
    // /param/u_orb_y   0.5
    let mut osc = OscParamReceiver::bind("127.0.0.1:9000")
        .map_err(|e| EngineError::GlCreate(format!("OSC bind failed: {e}")))?;

    // Feedback tunables
    let mut u_decay: f32 = 0.9992;
    let mut u_gain: f32 = 1.02;
    let mut u_mix: f32 = 0.9;
    let mut u_spin: f32 = 0.35;
    let mut u_zoom: f32 = 1.00;
    let mut u_blur: f32 = 0.6; // 0..2 (directional smear amount)

    // Orb tunables (normalized coords)
    let mut u_orb_size: f32 = 0.22;
    let mut u_orb_x: f32 = 0.5;
    let mut u_orb_y: f32 = 0.5;

    // --- Orb source shader ---
    let orb_frag = r#"
#version 330 core
in vec2 v_uv;
out vec4 fragColor;

uniform vec2  u_resolution;
uniform float u_time;

uniform float u_orb_size; // ~0.05..0.5 (normalized)
uniform vec2  u_orb_pos;  // 0..1

void main() {
    vec2 uv = clamp(v_uv * 0.5, 0.0, 1.0);
    vec2 p = uv - u_orb_pos;

    float d = length(p);
    float core = smoothstep(u_orb_size, 0.0, d);
    core = pow(core, 1.6); // tighter core
    float ring = smoothstep(u_orb_size*1.25, u_orb_size*0.9, d) * 0.6;

    float pulse = 0.6 + 0.4*sin(u_time*1.5);
    vec3 col = vec3(0.20, 0.70, 1.00) * (core * (0.9 + 0.6*pulse) + ring);

    // add subtle internal detail
    float swirl = sin((p.x*22.0 + p.y*18.0) + u_time*2.2) * 0.22;
    col += swirl * core;

    fragColor = vec4(max(col, vec3(0.0)), 1.0);
}
"#;

    // --- Dual-input feedback shader ---
    let fb_frag = r#"
#version 330 core
in vec2 v_uv;
out vec4 fragColor;

uniform sampler2D u_feedback;
uniform sampler2D u_orb;

uniform vec2  u_resolution;
uniform float u_time;

uniform float u_decay;
uniform float u_gain;
uniform float u_mix;   // injection amount for orb
uniform float u_spin;  // radians/sec
uniform float u_zoom;  // >1 zoom in, <1 zoom out
uniform float u_blur;  // 0..2 : directional smear amount (waaavepool-ish)

vec3 fb(vec2 uv) { return texture(u_feedback, clamp(uv, 0.0, 1.0)).rgb; }

void main() {
    vec2 uv = clamp(v_uv * 0.5, 0.0, 1.0);

    // feedback transform: zoom + rotate around center
    vec2 p = uv - 0.5;
    p /= max(u_zoom, 0.0001);
    float a = u_spin * u_time;
    mat2 R = mat2(cos(a), -sin(a), sin(a), cos(a));
    p = R * p;
    vec2 uv_t = p + 0.5;

    // ---- Directional smear (one-sided) ----
    // Direction follows the current rotation angle; this tends to produce "melt" trails.
    vec2 px = 1.0 / max(u_resolution, vec2(1.0));
    vec2 dir = normalize(vec2(cos(a), sin(a)));
    float amt = clamp(u_blur, 0.0, 2.0);

    // smear length in pixels (scaled)
    vec2 stepv = dir * px * (1.0 + 6.0 * amt);

    // One-sided accumulation "behind" the motion direction
    vec3 acc = fb(uv_t);
    acc += fb(uv_t - stepv * 1.0);
    acc += fb(uv_t - stepv * 2.0);
    acc += fb(uv_t - stepv * 3.0);
    acc += fb(uv_t - stepv * 4.0);
    acc += fb(uv_t - stepv * 6.0);

    // Weighted average (slightly favor newest sample)
    acc = acc * (1.0 / 6.0);

    vec3 prev = acc * u_decay * u_gain;

    // source (orb) sampled in screen space (no transform)
    vec3 src = texture(u_orb, uv).rgb;

    // Trails: additive injection
    vec3 outc = prev + src * u_mix;

    fragColor = vec4(clamp(outc, 0.0, 1.0), 1.0);
}
"#;

    let orb_prog = unsafe { compile_program(&gl, FULLSCREEN_VERT, orb_frag)? };
    let fb_prog = unsafe { compile_program(&gl, FULLSCREEN_VERT, fb_frag)? };

    // Targets
    let mut ping = unsafe { PingPongTarget::new(&gl, size.width as i32, size.height as i32)? };
    let mut orb_rt = unsafe { create_render_target(&gl, size.width as i32, size.height as i32)? };

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
                        gl.delete_texture(orb_rt.tex);
                        gl.delete_framebuffer(orb_rt.fbo);
                        orb_rt = create_render_target(&gl, w as i32, h as i32).unwrap();
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

                // Apply OSC updates
                for (name, val) in osc.poll() {
                    match name.as_str() {
                        "u_decay" => u_decay = val,
                        "u_gain" => u_gain = val,
                        "u_mix" => u_mix = val,
                        "u_spin" => u_spin = val,
                        "u_zoom" => u_zoom = val,
                        "u_blur" => u_blur = val,
                        "u_orb_size" => u_orb_size = val,
                        "u_orb_x" => u_orb_x = val,
                        "u_orb_y" => u_orb_y = val,
                        _ => {}
                    }
                }

                unsafe {
                    // 1) Render orb source into orb_rt
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(orb_rt.fbo));
                    gl.viewport(0, 0, w, h);
                    gl.clear_color(0.0, 0.0, 0.0, 1.0);
                    gl.clear(glow::COLOR_BUFFER_BIT);

                    gl.use_program(Some(orb_prog));
                    if let Some(loc) = gl.get_uniform_location(orb_prog, "u_time") {
                        gl.uniform_1_f32(Some(&loc), t);
                    }
                    if let Some(loc) = gl.get_uniform_location(orb_prog, "u_resolution") {
                        gl.uniform_2_f32(Some(&loc), w as f32, h as f32);
                    }
                    if let Some(loc) = gl.get_uniform_location(orb_prog, "u_orb_size") {
                        gl.uniform_1_f32(Some(&loc), u_orb_size);
                    }
                    if let Some(loc) = gl.get_uniform_location(orb_prog, "u_orb_pos") {
                        gl.uniform_2_f32(Some(&loc), u_orb_x, u_orb_y);
                    }
                    fs_tri.draw(&gl);
                    gl.use_program(None);
                    gl.bind_framebuffer(glow::FRAMEBUFFER, None);

                    // 2) Render feedback into ping.next (sampling ping.prev + orb_rt)
                    let next = ping.next_target();
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(next.fbo));
                    gl.viewport(0, 0, w, h);

                    gl.use_program(Some(fb_prog));
                    if let Some(loc) = gl.get_uniform_location(fb_prog, "u_time") {
                        gl.uniform_1_f32(Some(&loc), t);
                    }
                    if let Some(loc) = gl.get_uniform_location(fb_prog, "u_resolution") {
                        gl.uniform_2_f32(Some(&loc), w as f32, h as f32);
                    }
                    if let Some(loc) = gl.get_uniform_location(fb_prog, "u_decay") {
                        gl.uniform_1_f32(Some(&loc), u_decay);
                    }
                    if let Some(loc) = gl.get_uniform_location(fb_prog, "u_gain") {
                        gl.uniform_1_f32(Some(&loc), u_gain);
                    }
                    if let Some(loc) = gl.get_uniform_location(fb_prog, "u_mix") {
                        gl.uniform_1_f32(Some(&loc), u_mix);
                    }
                    if let Some(loc) = gl.get_uniform_location(fb_prog, "u_spin") {
                        gl.uniform_1_f32(Some(&loc), u_spin);
                    }
                    if let Some(loc) = gl.get_uniform_location(fb_prog, "u_zoom") {
                        gl.uniform_1_f32(Some(&loc), u_zoom);
                    }
                    if let Some(loc) = gl.get_uniform_location(fb_prog, "u_blur") {
                        gl.uniform_1_f32(Some(&loc), u_blur);
                    }

                    // bind feedback texture on unit 0
                    gl.active_texture(glow::TEXTURE0);
                    gl.bind_texture(glow::TEXTURE_2D, Some(ping.prev_tex()));
                    if let Some(loc) = gl.get_uniform_location(fb_prog, "u_feedback") {
                        gl.uniform_1_i32(Some(&loc), 0);
                    }

                    // bind orb texture on unit 1
                    gl.active_texture(glow::TEXTURE1);
                    gl.bind_texture(glow::TEXTURE_2D, Some(orb_rt.tex));
                    if let Some(loc) = gl.get_uniform_location(fb_prog, "u_orb") {
                        gl.uniform_1_i32(Some(&loc), 1);
                    }

                    fs_tri.draw(&gl);

                    gl.bind_texture(glow::TEXTURE_2D, None);
                    gl.active_texture(glow::TEXTURE0);
                    gl.bind_texture(glow::TEXTURE_2D, None);

                    gl.use_program(None);
                    gl.bind_framebuffer(glow::FRAMEBUFFER, None);

                    // Blit next to window
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
                ping.swap();
            }

            _ => {}
        }
    });
}
