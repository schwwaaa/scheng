use std::num::NonZeroU32;
use std::time::Instant;

use glow::HasContext;

use glutin::config::ConfigTemplateBuilder;

use glutin::context::{
    ContextApi, ContextAttributesBuilder, NotCurrentContext, NotCurrentGlContextSurfaceAccessor,
    Version,
};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::prelude::GlSurface;
use glutin::surface::{SurfaceAttributesBuilder, SwapInterval, WindowSurface};

use glutin_winit::DisplayBuilder;
use raw_window_handle::HasRawWindowHandle;
use winit::dpi::PhysicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

use scheng_graph as graph;

// IMPORTANT: OutputSink + ExecOutputs live in runtime-glow.
use scheng_runtime_glow as rt;

// -------------------------
// Constants
// -------------------------

const RENDER_W: i32 = 1920;
const RENDER_H: i32 = 1080;

const OUT_PROGRAM: &str = "program";
const OUT_PREVIEW: &str = "preview";

// -------------------------
// Present (preview monitor)
// -------------------------

struct PresentBlitSink {
    w: i32,
    h: i32,
}

impl rt::OutputSink for PresentBlitSink {
    fn consume(&mut self, gl: &glow::Context, out: &rt::ExecOutput) {
        unsafe {
            let win_w = self.w.max(1);
            let win_h = self.h.max(1);

            let src_w = out.width.max(1);
            let src_h = out.height.max(1);

            let src_aspect = src_w as f32 / src_h as f32;
            let win_aspect = win_w as f32 / win_h as f32;

            let (dst_w, dst_h) = if win_aspect > src_aspect {
                let dh = win_h;
                let dw = (dh as f32 * src_aspect).round() as i32;
                (dw.max(1), dh)
            } else {
                let dw = win_w;
                let dh = (dw as f32 / src_aspect).round() as i32;
                (dw, dh.max(1))
            };

            let x0 = (win_w - dst_w) / 2;
            let y0 = (win_h - dst_h) / 2;
            let x1 = x0 + dst_w;
            let y1 = y0 + dst_h;

            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(out.fbo));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
            gl.blit_framebuffer(
                0,
                0,
                src_w,
                src_h,
                x0,
                y0,
                x1,
                y1,
                glow::COLOR_BUFFER_BIT,
                glow::LINEAR,
            );
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
        }
    }
}

struct PreviewRouteSink {
    present: PresentBlitSink,
}

impl PreviewRouteSink {
    fn new(w: i32, h: i32) -> Self {
        Self {
            present: PresentBlitSink { w, h },
        }
    }

    fn resize(&mut self, w: i32, h: i32) {
        self.present.w = w;
        self.present.h = h;
    }
}

impl rt::OutputSink for PreviewRouteSink {
    fn consume(&mut self, gl: &glow::Context, out: &rt::ExecOutput) {
        rt::OutputSink::consume(&mut self.present, gl, out);
    }
}

// -------------------------
// Program route (Syphon + taps)
// -------------------------

struct ProgramRouteSink {
    history: rt::HistoryTapSink,
    readback: rt::ReadbackSink,
    #[cfg(feature = "syphon")]
    syphon: rt::SyphonSink,
}

impl ProgramRouteSink {
    fn new() -> Result<Self, rt::EngineError> {
        Ok(Self {
            history: rt::HistoryTapSink::new(16),
            readback: rt::ReadbackSink::new(60),
            #[cfg(feature = "syphon")]
            syphon: rt::SyphonSink::new("scheng")?,
        })
    }
}

impl rt::OutputSink for ProgramRouteSink {
    fn consume(&mut self, gl: &glow::Context, out: &rt::ExecOutput) {
        rt::OutputSink::consume(&mut self.history, gl, out);
        rt::OutputSink::consume(&mut self.readback, gl, out);
        #[cfg(feature = "syphon")]
        rt::OutputSink::consume(&mut self.syphon, gl, out);
    }
}

// -------------------------
// Shaders
// -------------------------

const FRAG_A: &str = r#"#version 330 core
uniform vec2 uResolution;
uniform float uTime;
out vec4 FragColor;
void main() {
    vec2 uv = gl_FragCoord.xy / uResolution.xy;
    float v = 0.5 + 0.5 * sin((uv.x * 10.0) + uTime * 1.3);
    FragColor = vec4(v, 0.1, 0.2, 1.0);
}
"#;

const FRAG_B: &str = r#"#version 330 core
uniform vec2 uResolution;
uniform float uTime;
out vec4 FragColor;
void main() {
    vec2 uv = gl_FragCoord.xy / uResolution.xy;
    float v = 0.5 + 0.5 * cos((uv.y * 12.0) - uTime * 0.9);
    FragColor = vec4(0.1, v, 0.8, 1.0);
}
"#;

// -------------------------
// Main
// -------------------------

fn main() {
    let event_loop = EventLoop::new();

    let window_builder = WindowBuilder::new()
        .with_title("scheng graph mixer builtin")
        .with_inner_size(PhysicalSize::new(960, 540));

    // IMPORTANT: glutin-winit expects ConfigTemplateBuilder, NOT a built ConfigTemplate.
    let template = ConfigTemplateBuilder::new()
        .with_alpha_size(8)
        .with_depth_size(0)
        .with_stencil_size(0);

    let display_builder = DisplayBuilder::new().with_window_builder(Some(window_builder));

    let (window, gl_config) = display_builder
        .build(&event_loop, template, |mut configs| configs.next().unwrap())
        .unwrap();

    let window = window.unwrap();
    let gl_display = gl_config.display();

    let context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .build(Some(window.raw_window_handle()));

    let not_current: NotCurrentContext =
        unsafe { gl_display.create_context(&gl_config, &context_attributes).unwrap() };

    let size = window.inner_size();
    let surface_attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        window.raw_window_handle(),
        NonZeroU32::new(size.width.max(1)).unwrap(),
        NonZeroU32::new(size.height.max(1)).unwrap(),
    );

    let gl_surface = unsafe {
        gl_display
            .create_window_surface(&gl_config, &surface_attrs)
            .unwrap()
    };

    let gl_context = not_current.make_current(&gl_surface).unwrap();

    gl_surface
        .set_swap_interval(&gl_context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()))
        .ok();

    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            gl_display.get_proc_address(std::ffi::CString::new(s).unwrap().as_c_str()) as *const _
        })
    };

    // -------------------------
    // Graph
    // -------------------------

    let mut g = graph::Graph::new();

    let pass_a = g.add_node(graph::NodeKind::ShaderPass);
    let pass_b = g.add_node(graph::NodeKind::ShaderPass);
    let mix = g.add_node(graph::NodeKind::Crossfade);

    // Step 5: multiple PixelsOut.
    // - one unnamed primary output (allowed)
    // - two named outputs ("program", "preview")
    let out_primary = g.add_node(graph::NodeKind::PixelsOut);
    let out_program = g.add_node(graph::NodeKind::PixelsOut);
    let out_preview = g.add_node(graph::NodeKind::PixelsOut);

    g.connect_named(pass_a, "out", mix, "a").unwrap();
    g.connect_named(pass_b, "out", mix, "b").unwrap();

    g.connect_named(mix, "out", out_primary, "in").unwrap();
    g.connect_named(mix, "out", out_program, "in").unwrap();
    g.connect_named(mix, "out", out_preview, "in").unwrap();

    let plan = g.compile().unwrap();

    let mut props = rt::NodeProps::default();

    // Output naming for the *named* PixelsOut nodes.
    props
        .output_names
        .insert(out_program, OUT_PROGRAM.to_string());
    props
        .output_names
        .insert(out_preview, OUT_PREVIEW.to_string());

    props.shader_sources.insert(
        pass_a,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: FRAG_A.to_string(),
            origin: Some("A".into()),
        },
    );

    props.shader_sources.insert(
        pass_b,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: FRAG_B.to_string(),
            origin: Some("B".into()),
        },
    );

    props
        .mixer_params
        .insert(mix, scheng_runtime::MixerParams { mix: 0.35 });

    let mut state = unsafe { rt::RuntimeState::new(&gl) }.unwrap();

    // -------------------------
    // Step 6 routing
    // -------------------------

    // Preview is local (so we can resize cleanly).
    let mut preview_sink = PreviewRouteSink::new(size.width as i32, size.height as i32);

    // Program routes via Patchbay (composable sinks).
    let program_sink = ProgramRouteSink::new().unwrap();

    let mut patchbay = rt::PatchbaySink::new();
    patchbay.add_route(OUT_PROGRAM, program_sink);

    let start = Instant::now();
    let mut frame_idx: u64 = 0;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                WindowEvent::Resized(new_size) => {
                    preview_sink.resize(new_size.width as i32, new_size.height as i32);

                    gl_surface.resize(
                        &gl_context,
                        NonZeroU32::new(new_size.width.max(1)).unwrap(),
                        NonZeroU32::new(new_size.height.max(1)).unwrap(),
                    );
                }
                _ => {}
            },
            Event::RedrawRequested(_) => {
                let frame = rt::FrameCtx {
                    time: start.elapsed().as_secs_f32(),
                    width: RENDER_W,
                    height: RENDER_H,
                    frame: frame_idx,
                };
                frame_idx += 1;

                let outs =
                    unsafe { rt::execute_plan_outputs(&gl, &g, &plan, &mut state, &props, frame) }
                        .unwrap();

                // Preview: explicitly consume the named preview output into the window.
                if let Some(preview_out) = outs.get(OUT_PREVIEW) {
                    rt::OutputSink::consume(&mut preview_sink, &gl, preview_out);
                } else {
                    // Fallback to primary output if preview was not produced (shouldn't happen in this example).
                    let main_out = outs.primary();
                    rt::OutputSink::consume(&mut preview_sink, &gl, main_out);
                }

                // Program: Patchbay routes program output into sinks (Syphon, readback, history, etc).
                patchbay.consume_named(&gl, &outs).unwrap();

                gl_surface.swap_buffers(&gl_context).unwrap();
            }
            Event::MainEventsCleared => window.request_redraw(),
            _ => {}
        }
    });
}
