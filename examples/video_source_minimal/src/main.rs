use std::num::NonZeroU32;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use glow::HasContext;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{
    ContextAttributesBuilder, NotCurrentGlContextSurfaceAccessor, PossiblyCurrentContext,
};
use glutin::display::GetGlDisplay;
use glutin::prelude::{GlConfig, GlDisplay, GlSurface};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasRawWindowHandle;
use winit::dpi::PhysicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::ControlFlow;

use scheng_control_osc::OscParamReceiver;
use scheng_graph as graph;
use scheng_input_video::{VideoConfig, VideoDecoder};
use scheng_runtime_glow as rt;

const WIN_W: u32 = 960;
const WIN_H: u32 = 540;

// Graph pass shader: passthrough ONLY.
const PASS_FRAG: &str = r#"#version 330 core
in vec2 v_uv;
out vec4 o;
uniform sampler2D iChannel0;
void main() { o = texture(iChannel0, v_uv); }
"#;

// Presenter shader: exposes u_gain for OSC control.
const PRESENT_FRAG: &str = r#"#version 330 core
in vec2 v_uv;
out vec4 o;
uniform sampler2D iChannel0;
uniform float u_gain;
void main() {
    vec4 c = texture(iChannel0, v_uv);
    c.rgb *= u_gain;
    o = c;
}
"#;

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

    unsafe fn present(
        &self,
        gl: &glow::Context,
        tex: glow::NativeTexture,
        w: i32,
        h: i32,
        gain: f32,
    ) {
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
        if let Some(loc) = gl.get_uniform_location(self.program, "u_gain") {
            gl.uniform_1_f32(Some(&loc), gain);
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
    let template =
        ConfigTemplateBuilder::new().with_alpha_size(8).with_depth_size(0).with_stencil_size(0);

    let display_builder = DisplayBuilder::new().with_window_builder(Some(
        winit::window::WindowBuilder::new()
            .with_title("scheng: video_source_minimal (OSC gain + speed)")
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

    let gl_display = gl_config.display();

    let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window_handle));
    let not_current_gl_context =
        unsafe { gl_display.create_context(&gl_config, &context_attributes).unwrap() };

    let size = window.inner_size();
    let attrs = glutin::surface::SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new()
        .build(
            raw_window_handle,
            NonZeroU32::new(size.width.max(1)).unwrap(),
            NonZeroU32::new(size.height.max(1)).unwrap(),
        );

    let gl_surface = unsafe { gl_display.create_window_surface(&gl_config, &attrs).unwrap() };
    let gl_context = not_current_gl_context.make_current(&gl_surface).unwrap();

    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            gl_display.get_proc_address(std::ffi::CStr::from_bytes_with_nul_unchecked(
                format!("{s}\0").as_bytes(),
            )) as *const _
        })
    };

    (window, gl_surface, gl_context, gl)
}

unsafe fn make_host_texture(gl: &glow::Context, w: i32, h: i32) -> glow::NativeTexture {
    let tex = gl.create_texture().unwrap();
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA8 as i32,
        w,
        h,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        None,
    );
    tex
}

unsafe fn upload_rgba_to_texture(
    gl: &glow::Context,
    tex: glow::NativeTexture,
    w: i32,
    h: i32,
    rgba: &[u8],
) {
    debug_assert_eq!(rgba.len(), (w * h * 4) as usize);
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_sub_image_2d(
        glow::TEXTURE_2D,
        0,
        0,
        0,
        w,
        h,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(rgba),
    );
}

fn print_usage_and_exit() -> ! {
    eprintln!(
        "Usage:
  scheng-example-video-source-minimal --config path/to/video.json [--osc-verbose]
  scheng-example-video-source-minimal --file path/to/video.mp4 [--w 640 --h 360 --fps 30] [--loop 0|1] [--ffmpeg /path/to/ffmpeg] [--osc-verbose]

Config JSON format:
{{
  \"file\": \"path/to/video.mp4\",
  \"width\": 640,
  \"height\": 360,
  \"fps\": 30,
  \"loop\": true,
  \"ffmpeg_path\": \"optional/explicit/ffmpeg\"
}}

Notes:
- ffmpeg resolution priority: --ffmpeg > config.ffmpeg_path > $scheng_FFMPEG > bundled vendor/ffmpeg > PATH
- OSC (binds 127.0.0.1:9000 when --osc-verbose is set):
    /u_gain  <float>   : brightness gain (default 1.0)
    /u_speed <float>   : playback speed multiplier vs configured fps (default 1.0, 0.0=pause)
"
    );
    std::process::exit(2);
}

fn parse_args() -> (VideoConfig, bool) {
    let mut args = std::env::args().skip(1);

    let mut config_path: Option<PathBuf> = None;
    let mut file: Option<String> = None;
    let mut width: Option<u32> = None;
    let mut height: Option<u32> = None;
    let mut fps: Option<u32> = None;
    let mut loop_flag: Option<bool> = None;
    let mut ffmpeg_path: Option<String> = None;
    let mut osc_verbose = false;

    while let Some(a) = args.next() {
        match a.as_str() {
            "--config" => config_path = args.next().map(PathBuf::from),
            "--file" => file = args.next(),
            "--w" => width = args.next().and_then(|s| s.parse().ok()),
            "--h" => height = args.next().and_then(|s| s.parse().ok()),
            "--fps" => fps = args.next().and_then(|s| s.parse().ok()),
            "--loop" => {
                loop_flag = args.next().map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
            }
            "--ffmpeg" => ffmpeg_path = args.next(),
            "--osc-verbose" => osc_verbose = true,
            "--help" | "-h" => print_usage_and_exit(),
            _ => {
                eprintln!("Unknown arg: {a}");
                print_usage_and_exit();
            }
        }
    }

    if let Some(p) = config_path {
        let text = std::fs::read_to_string(&p).unwrap_or_else(|e| {
            eprintln!("Failed to read config {p:?}: {e}");
            std::process::exit(2);
        });

        let mut cfg: VideoConfig = serde_json::from_str(&text).unwrap_or_else(|e| {
            eprintln!("Failed to parse config {p:?}: {e}");
            std::process::exit(2);
        });

        // CLI overrides
        if let Some(f) = file {
            cfg.file = f;
        }
        if let Some(w) = width {
            cfg.width = w;
        }
        if let Some(h) = height {
            cfg.height = h;
        }
        if let Some(x) = fps {
            cfg.fps = x;
        }
        if let Some(l) = loop_flag {
            cfg.r#loop = l;
        }
        if let Some(ff) = ffmpeg_path {
            cfg.ffmpeg_path = Some(ff);
        }

        if cfg.width == 0 || cfg.height == 0 || cfg.fps == 0 {
            eprintln!("width/height/fps must be > 0");
            std::process::exit(2);
        }

        return (cfg, osc_verbose);
    }

    let Some(f) = file else {
        print_usage_and_exit();
    };

    let cfg = VideoConfig {
        file: f,
        width: width.unwrap_or(640),
        height: height.unwrap_or(360),
        fps: fps.unwrap_or(30),
        r#loop: loop_flag.unwrap_or(true),
        ffmpeg_path,
    };

    if cfg.width == 0 || cfg.height == 0 || cfg.fps == 0 {
        eprintln!("width/height/fps must be > 0");
        std::process::exit(2);
    }

    (cfg, osc_verbose)
}

fn clamp_f32(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

fn main() {
    let (cfg, osc_verbose) = parse_args();
    let base_fps = cfg.fps.max(1) as f32;

    let mut decoder = VideoDecoder::from_config(cfg).expect("create decoder");

    // Optional OSC listener. If bind fails, log once and continue (video still works).
    let mut osc: Option<OscParamReceiver> = if osc_verbose {
        match OscParamReceiver::bind("127.0.0.1:9000") {
            Ok(o) => {
                println!("OSC listening on 127.0.0.1:9000 (drives u_gain + u_speed)");
                Some(o)
            }
            Err(e) => {
                eprintln!("Failed to bind OSC on 127.0.0.1:9000: {e}");
                None
            }
        }
    } else {
        None
    };

    let event_loop = winit::event_loop::EventLoop::new();
    let (window, gl_surface, gl_context, gl) = make_gl(&event_loop);

    // --- Graph ---
    let mut g = graph::Graph::new();
    let tex_in = g.add_node(graph::NodeKind::TextureInputPass);
    let pass = g.add_node(graph::NodeKind::ShaderPass);
    let out = g.add_node(graph::NodeKind::PixelsOut);

    g.connect_named(tex_in, "out", pass, "in")
        .expect("connect tex->pass");
    g.connect_named(pass, "out", out, "in")
        .expect("connect pass->out");

    let plan = g.compile().expect("compile plan");

    let mut props = rt::NodeProps::default();
    props.output_names.insert(out, "preview".into());
    props.shader_sources.insert(
        pass,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: PASS_FRAG.to_string(),
            origin: Some("video_source_minimal:passthrough".into()),
        },
    );

    let mut state = unsafe { rt::RuntimeState::new(&gl).expect("rt state") };
    let presenter = unsafe { Presenter::new(&gl).expect("presenter") };

    // texture allocated once we see first frame
    let mut host_tex: Option<glow::NativeTexture> = None;
    let mut tex_w: i32 = 0;
    let mut tex_h: i32 = 0;

    let t0 = Instant::now();
    let mut frame_index: u64 = 0;

    // OSC-controlled parameters
    let mut u_gain: f32 = 1.0;
    let mut u_speed: f32 = 1.0;

    // Playback pacing based on configured fps * u_speed
    let mut next_decode_due = Instant::now();

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
                _ => {}
            },
            Event::MainEventsCleared => window.request_redraw(),
            Event::RedrawRequested(_) => {
                let elapsed = t0.elapsed().as_secs_f32();

                // OSC → update u_gain and u_speed
                if let Some(osc) = osc.as_mut() {
                    for (name, val) in osc.poll() {
                        if osc_verbose {
                            println!("OSC recv: {} = {}", name, val);
                        }
                        if name == "u_gain" {
                            u_gain = clamp_f32(val, 0.0, 8.0);
                        } else if name == "u_speed" {
                            // 0 = pause, >0 scales fps pacing
                            u_speed = clamp_f32(val, 0.0, 4.0);
                            next_decode_due = Instant::now();
                        }
                    }
                }

                // Decode pacing: only pull frames when due (or pause if u_speed==0)
                if u_speed > 0.0 {
                    let effective_fps = base_fps * u_speed;
                    let dt = Duration::from_secs_f32(1.0 / effective_fps.max(0.001));

                    // Catch-up loop (bounded) so >1.0 speed can advance.
                    let mut steps = 0;
                    while Instant::now() >= next_decode_due && steps < 5 {
                        next_decode_due += dt;
                        steps += 1;

                        if let Ok(frame) = decoder.poll_rgba() {
                            let w = frame.width as i32;
                            let h = frame.height as i32;

                            if host_tex.is_none() || w != tex_w || h != tex_h {
                                if let Some(old) = host_tex.take() {
                                    unsafe { gl.delete_texture(old) };
                                }
                                tex_w = w;
                                tex_h = h;
                                host_tex = Some(unsafe { make_host_texture(&gl, tex_w, tex_h) });
                            }

                            if let Some(tex) = host_tex {
                                unsafe {
                                    upload_rgba_to_texture(&gl, tex, tex_w, tex_h, &frame.bytes);
                                }
                            }
                        }
                    }
                }

                let size = window.inner_size();
                let w = size.width as i32;
                let h = size.height as i32;

                let frame_ctx = rt::FrameCtx {
                    width: w,
                    height: h,
                    time: elapsed,
                    frame: frame_index,
                };
                frame_index = frame_index.wrapping_add(1);

                if let Some(tex) = host_tex {
                    props.texture_inputs.insert(tex_in, tex);

                    unsafe {
                        let outs =
                            rt::execute_plan_outputs(&gl, &g, &plan, &mut state, &props, frame_ctx)
                                .expect("execute");
                        let main_out = outs.primary;
                        presenter.present(&gl, main_out.tex, w, h, u_gain);
                        gl_surface.swap_buffers(&gl_context).unwrap();
                    }
                }
            }
            _ => {}
        }
    });
}
