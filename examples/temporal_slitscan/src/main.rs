use glow::HasContext;
use scheng_control_osc::OscReceiver;
use scheng_runtime_glow::{
    compile_program, create_render_target, EngineError, ExecOutput, FullscreenTriangle, HistoryTapSink,
    ReadbackSink, FULLSCREEN_VERT,
};
use scheng_runtime_glow::OutputSink;

use std::num::NonZeroU32;
use std::time::Instant;

use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

use glutin::display::GetGlDisplay;
use glutin::prelude::*;

// raw-window-handle 0.5 traits
use raw_window_handle::HasRawWindowHandle;

const RING_N: usize = 16;

fn main() {
    if let Err(e) = run() {
        eprintln!("[temporal_slitscan] error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), EngineError> {
    let event_loop = EventLoop::new();

    let window_builder = WindowBuilder::new()
        .with_title("scheng-sdk: TemporalRing + temporal slit-scan")
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

    // OSC: 127.0.0.1:9000
    // /param/u_slices  64
    // /param/u_span    16
    // /param/u_offset  0.0
    // /param/u_quant   1.0
    // /param/u_axis    0.0 (0=x, 1=y)
    let mut osc = OscReceiver::bind("127.0.0.1:9000")
        .map_err(|e| EngineError::GlCreate(format!("OSC bind failed: {e}")))?;

    let mut u_slices: f32 = 64.0;
    let mut u_span: f32 = 16.0;
    let mut u_offset: f32 = 0.0;
    let mut u_quant: f32 = 1.0;
    let mut u_axis: f32 = 0.0;

    let mut src_rt = unsafe { create_render_target(&gl, size.width as i32, size.height as i32)? };
    let mut history = HistoryTapSink::new(RING_N);
    let mut readback = ReadbackSink::new(60);

    let src_frag = r#"
#version 330 core
in vec2 v_uv;
out vec4 fragColor;
uniform vec2 u_resolution;
uniform float u_time;

float hash(vec2 p){
    p = fract(p*vec2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

void main(){
    vec2 uv = clamp(v_uv*0.5, 0.0, 1.0);
    vec2 c = vec2(0.5 + 0.18*sin(u_time*0.9), 0.5 + 0.12*cos(u_time*1.1));
    float d = length(uv - c);
    float blob = smoothstep(0.24, 0.0, d);

    float bars = smoothstep(0.49, 0.5, abs(sin((uv.y*280.0) + u_time*2.0))) * 0.18;
    float n = (hash(floor(uv*u_resolution*0.25)) - 0.5) * 0.07;

    vec3 col = vec3(0.0);
    col += vec3(0.2, 0.7, 1.0) * blob;
    col += vec3(1.0, 0.35, 0.2) * bars;
    col += vec3(n);

    fragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
"#;

    // sampler array indexed via if-ladder for portability across GLSL variants.
    let slitscan_frag = format!(
        r#"
#version 330 core
in vec2 v_uv;
out vec4 fragColor;

uniform vec2 u_resolution;
uniform float u_time;

uniform float u_slices;
uniform float u_span;
uniform float u_offset;
uniform float u_quant;
uniform float u_axis;

uniform sampler2D u_frames[{RING_N}];

vec3 frame_sample(int i, vec2 uv) {{
    if (i <= 0) return texture(u_frames[0], uv).rgb;
{ifs}
    return texture(u_frames[{last}], uv).rgb;
}}

void main() {{
    vec2 uv = clamp(v_uv*0.5, 0.0, 1.0);

    float axisv = (u_axis < 0.5) ? uv.x : uv.y;

    float slices = max(u_slices, 1.0);
    float span = clamp(u_span, 1.0, float({RING_N}));

    float band = floor(axisv * slices);
    float taddr = band + u_offset * slices;

    float q = max(u_quant, 1.0);
    taddr = floor(taddr / q) * q;

    float idxf = mod(taddr, span);
    int idx = int(clamp(idxf, 0.0, span-1.0));

    vec3 col = frame_sample(idx, uv);
    fragColor = vec4(col, 1.0);
}}
"#,
        RING_N = RING_N,
        last = RING_N - 1,
        ifs = (1..RING_N)
            .map(|k| format!("    if (i == {k}) return texture(u_frames[{k}], uv).rgb;"))
            .collect::<Vec<_>>()
            .join(
                "
"
            ),
    );

    let src_prog = unsafe { compile_program(&gl, FULLSCREEN_VERT, src_frag)? };
    let slitscan_prog = unsafe { compile_program(&gl, FULLSCREEN_VERT, &slitscan_frag)? };

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
                        gl.delete_texture(src_rt.tex);
                        gl.delete_framebuffer(src_rt.fbo);
                        src_rt = create_render_target(&gl, w as i32, h as i32).unwrap();
                        history.destroy(&gl);
                        history = HistoryTapSink::new(RING_N);
                        readback.clear();
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

                for (name, val) in osc.poll() {
                    match name.as_str() {
                        "u_slices" => u_slices = val,
                        "u_span" => u_span = val,
                        "u_offset" => u_offset = val,
                        "u_quant" => u_quant = val,
                        "u_axis" => u_axis = val,
                        _ => {}
                    }
                }

                unsafe {
                    // 1) render source into src_rt
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(src_rt.fbo));
                    gl.viewport(0, 0, w, h);
                    gl.clear_color(0.0, 0.0, 0.0, 1.0);
                    gl.clear(glow::COLOR_BUFFER_BIT);

                    gl.use_program(Some(src_prog));
                    if let Some(loc) = gl.get_uniform_location(src_prog, "u_time") {
                        gl.uniform_1_f32(Some(&loc), t);
                    }
                    if let Some(loc) = gl.get_uniform_location(src_prog, "u_resolution") {
                        gl.uniform_2_f32(Some(&loc), w as f32, h as f32);
                    }
                    fs_tri.draw(&gl);
                    gl.use_program(None);
                    gl.bind_framebuffer(glow::FRAMEBUFFER, None);

                    // 2) push into TemporalRing
                    let out = ExecOutput { tex: src_rt.tex, fbo: src_rt.fbo, width: w, height: h };
                    history.consume(&gl, &out);
                    readback.consume(&gl, &out);

                    // 3) output: time addressing via slit-scan
                    gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                    gl.viewport(0, 0, w, h);

                    gl.use_program(Some(slitscan_prog));
                    if let Some(loc) = gl.get_uniform_location(slitscan_prog, "u_time") {
                        gl.uniform_1_f32(Some(&loc), t);
                    }
                    if let Some(loc) = gl.get_uniform_location(slitscan_prog, "u_resolution") {
                        gl.uniform_2_f32(Some(&loc), w as f32, h as f32);
                    }
                    if let Some(loc) = gl.get_uniform_location(slitscan_prog, "u_slices") {
                        gl.uniform_1_f32(Some(&loc), u_slices);
                    }
                    if let Some(loc) = gl.get_uniform_location(slitscan_prog, "u_span") {
                        gl.uniform_1_f32(Some(&loc), u_span);
                    }
                    if let Some(loc) = gl.get_uniform_location(slitscan_prog, "u_offset") {
                        gl.uniform_1_f32(Some(&loc), u_offset);
                    }
                    if let Some(loc) = gl.get_uniform_location(slitscan_prog, "u_quant") {
                        gl.uniform_1_f32(Some(&loc), u_quant);
                    }
                    if let Some(loc) = gl.get_uniform_location(slitscan_prog, "u_axis") {
                        gl.uniform_1_f32(Some(&loc), u_axis);
                    }

                    // Bind frames_ago textures to units 0..RING_N-1
                    for k in 0..RING_N {
                        gl.active_texture(glow::TEXTURE0 + (k as u32));
                        // If history hasn't filled yet, fall back to the current source frame.
                        let tex = history.tex_at(k).unwrap_or(src_rt.tex);
                        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                    }
                    if let Some(loc) = gl.get_uniform_location(slitscan_prog, "u_frames") {
                        let units: Vec<i32> = (0..RING_N as i32).collect();
                        gl.uniform_1_i32_slice(Some(&loc), &units);
                    }

                    fs_tri.draw(&gl);

                    for k in 0..RING_N {
                        gl.active_texture(glow::TEXTURE0 + (k as u32));
                        gl.bind_texture(glow::TEXTURE_2D, None);
                    }
                    gl.active_texture(glow::TEXTURE0);
                    gl.use_program(None);
                }

                gl_surface.swap_buffers(&gl_context).unwrap();
            }

            _ => {}
        }
    });
}
