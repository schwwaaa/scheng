use std::io::Read;
use std::num::NonZeroU32;
use std::process::{Command, Stdio};
use std::time::Instant;

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

use scheng_graph as graph;
use scheng_runtime_glow as rt;

use scheng_input_webcam::Webcam;

const WIN_W: u32 = 960;
const WIN_H: u32 = 540;

// Graph pass shader: passthrough ONLY.
// (Do NOT rotate here, or you'll cancel out with the presenter.)
const PASS_FRAG: &str = r#"#version 330 core
in vec2 v_uv;
out vec4 o;
uniform sampler2D iChannel0;
void main() { o = texture(iChannel0, v_uv); }
"#;

// Presenter shader for nokhwa backend: 180° rotation (flip X + Y).
const PRESENT_FRAG_NOKHWA: &str = r#"#version 330 core
in vec2 v_uv;
out vec4 o;
uniform sampler2D iChannel0;
void main() {
    vec2 uv = vec2(1.0 - v_uv.x, 1.0 - v_uv.y);
    o = texture(iChannel0, uv);
}
"#;

// Presenter shader for ffmpeg backend: flip Y only (no horizontal mirror).
const PRESENT_FRAG_FFMPEG: &str = r#"#version 330 core
in vec2 v_uv;
out vec4 o;
uniform sampler2D iChannel0;
void main() {
    vec2 uv = vec2(v_uv.x, 1.0 - v_uv.y);
    o = texture(iChannel0, uv);
}
"#;

// Simple ffmpeg/avfoundation capture backend just for this example.
struct FfmpegDevice {
    width: u32,
    height: u32,
    frame_len: usize,
    child: std::process::Child,
    stdout: std::process::ChildStdout,
    buffer: Vec<u8>,
    eof_logged: bool,
}

impl Drop for FfmpegDevice {
    fn drop(&mut self) {
        // Ensure the ffmpeg process is terminated so the capture device is released.
        // This prevents the "works every second run" device-lock behavior.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl FfmpegDevice {
    fn new(device_index: u32, width: u32, height: u32) -> Result<Self, String> {
        // AVFoundation input spec supports "VIDEO_INDEX:AUDIO_INDEX".
        // We explicitly request video only to avoid audio-device binding issues and
        // reduce nondeterministic fallback behavior.
        let input_spec = format!("{device_index}:none");

        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-f")
            .arg("avfoundation")
            // avfoundation is real-time; no -re needed
            .arg("-framerate")
            .arg("30")
            .arg("-video_size")
            .arg(format!("{}x{}", width, height))
            .arg("-i")
            .arg(input_spec)
            .arg("-pix_fmt")
            .arg("rgba")
            .arg("-f")
            .arg("rawvideo")
            .arg("pipe:1")
            .stdout(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| format!("spawn ffmpeg: {e}"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "ffmpeg stdout not piped".to_string())?;

        let frame_len = (width as usize) * (height as usize) * 4;
        let buffer = vec![0u8; frame_len];

        Ok(Self {
            width,
            height,
            frame_len,
            child,
            stdout,
            buffer,
            eof_logged: false,
        })
    }

    fn poll_rgba(&mut self) -> Option<(u32, u32, Vec<u8>)> {
        if self.frame_len == 0 {
            return None;
        }

        match self.stdout.read_exact(&mut self.buffer) {
            Ok(()) => Some((self.width, self.height, self.buffer.clone())),
            Err(e) => {
                if !self.eof_logged {
                    eprintln!("ffmpeg device capture ended or failed: {e}");
                    self.eof_logged = true;
                }
                None
            }
        }
    }
}

// Unified frame source: either WebCam (nokhwa) or ffmpeg device.
enum FrameSource {
    Webcam(Webcam),
    Ffmpeg(FfmpegDevice),
}

impl FrameSource {
    fn poll_rgba(&mut self) -> Option<(u32, u32, Vec<u8>)> {
        match self {
            FrameSource::Webcam(cam) => cam
                .poll_rgba()
                .ok()
                .map(|f| (f.width, f.height, f.bytes)),
            FrameSource::Ffmpeg(dev) => dev.poll_rgba(),
        }
    }
}

struct Presenter {
    tri: rt::FullscreenTriangle,
    program: glow::NativeProgram,
}

impl Presenter {
    // Minimal change: allow choosing the presenter frag shader.
    unsafe fn new(gl: &glow::Context, frag_src: &str) -> Result<Self, rt::EngineError> {
        let tri = rt::FullscreenTriangle::new(gl)?;
        let program = rt::compile_program(gl, rt::FULLSCREEN_VERT, frag_src)?;
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
    let template =
        ConfigTemplateBuilder::new().with_alpha_size(8).with_depth_size(0).with_stencil_size(0);

    let display_builder = DisplayBuilder::new().with_window_builder(Some(
        winit::window::WindowBuilder::new()
            .with_title("scheng: video_device_capture_macos (Step 13.0)")
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
        "Usage:\n  scheng-example-video-device-capture-macos [--list-devices] [--backend nokhwa|ffmpeg] [--device-index N] [--w 640 --h 480]\n\nOptions:\n  --list-devices    List available video devices and exit\n  --backend BACKEND Backend to use: 'nokhwa' (default) or 'ffmpeg'\n  --device-index N  Index of the video capture device (default: 0)\n  --w WIDTH         Requested capture width in pixels (default: 640)\n  --h HEIGHT        Requested capture height in pixels (default: 480)\n  --help, -h        Show this help and exit\n\nNotes:\n- Devices include built-in webcams and USB video capture devices (HDMI/composite)\n- Resolution is best-effort; the driver may choose a nearby mode\n- This example is macOS-first and uses scheng-input-webcam (nokhwa) or ffmpeg/avfoundation\n"
    );
    std::process::exit(2);
}

#[derive(Clone, Debug)]
struct AppConfig {
    device_index: u32,
    width: u32,
    height: u32,
    list_devices: bool,
    backend: String, // \"nokhwa\" or \"ffmpeg\"
}

fn parse_args() -> AppConfig {
    let mut args = std::env::args().skip(1);

    let mut device_index: Option<u32> = None;
    let mut width: Option<u32> = None;
    let mut height: Option<u32> = None;
    let mut list_devices = false;
    let mut backend: Option<String> = None;

    while let Some(a) = args.next() {
        match a.as_str() {
            "--device-index" => {
                device_index = args.next().and_then(|s| s.parse().ok());
            }
            "--w" => {
                width = args.next().and_then(|s| s.parse().ok());
            }
            "--h" => {
                height = args.next().and_then(|s| s.parse().ok());
            }
            "--list-devices" => {
                list_devices = true;
            }
            "--backend" => {
                backend = args.next();
            }
            "--help" | "-h" => print_usage_and_exit(),
            _ => {
                eprintln!("Unknown arg: {a}");
                print_usage_and_exit();
            }
        }
    }

    AppConfig {
        device_index: device_index.unwrap_or(0),
        width: width.unwrap_or(640),
        height: height.unwrap_or(480),
        list_devices,
        backend: backend.unwrap_or_else(|| "nokhwa".to_string()),
    }
}

fn list_devices_and_exit() -> ! {
    #[cfg(not(target_os = "macos"))]
    {
        eprintln!(
            "Device listing in this example is currently only wired for macOS/native backend."
        );
        std::process::exit(1);
    }

    #[cfg(target_os = "macos")]
    {
        use nokhwa::{native_api_backend, query};

        println!("--- nokhwa (native) devices ---");
        let api = match native_api_backend() {
            Some(api) => api,
            None => {
                eprintln!(
                    "No native camera API backend available (nokhwa::native_api_backend returned None)."
                );
                std::process::exit(1);
            }
        };

        match query(api) {
            Ok(devices) => {
                if devices.is_empty() {
                    eprintln!("No video devices detected via nokhwa.");
                } else {
                    for (i, info) in devices.iter().enumerate() {
                        println!("[{i}] {info:?}");
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to query devices via nokhwa: {e}");
            }
        }

        println!("\n--- ffmpeg avfoundation devices ---");
        let _ = Command::new("ffmpeg")
            .args(["-f", "avfoundation", "-list_devices", "true", "-i", ""])
            .status();

        std::process::exit(0);
    }
}

fn main() {
    let cfg = parse_args();

    if cfg.list_devices {
        list_devices_and_exit();
    }

    let event_loop = winit::event_loop::EventLoop::new();
    let (window, gl_surface, gl_context, gl) = make_gl(&event_loop);

    // --- Graph ---
    let mut g = graph::Graph::new();
    let tex_in = g.add_node(graph::NodeKind::TextureInputPass);
    let pass = g.add_node(graph::NodeKind::ShaderPass);
    let out = g.add_node(graph::NodeKind::PixelsOut);

    g.connect_named(tex_in, "out", pass, "in").expect("connect tex->pass");
    g.connect_named(pass, "out", out, "in").expect("connect pass->out");

    let plan = g.compile().expect("compile plan");

    let mut props = rt::NodeProps::default();
    props.output_names.insert(out, "preview".into());

    // Use PASS_FRAG here (no flip).
    props.shader_sources.insert(
        pass,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: PASS_FRAG.to_string(),
            origin: Some("video_device_capture_macos:passthrough".into()),
        },
    );

    let mut state = unsafe { rt::RuntimeState::new(&gl).expect("rt state") };

    // Choose presenter orientation based on backend.
    let presenter_frag = match cfg.backend.as_str() {
        "ffmpeg" => PRESENT_FRAG_FFMPEG,
        _ => PRESENT_FRAG_NOKHWA,
    };
    let presenter = unsafe { Presenter::new(&gl, presenter_frag).expect("presenter") };

    // --- Video device (webcam / capture) ---
    let mut source = match cfg.backend.as_str() {
        "nokhwa" => {
            println!(
                "Using backend=nokhwa, device index {}, {}x{}",
                cfg.device_index, cfg.width, cfg.height
            );
            let cam = match Webcam::new(cfg.device_index, cfg.width, cfg.height) {
                Ok(cam) => cam,
                Err(e) => {
                    eprintln!(
                        "Failed to open video device via nokhwa (index {}): {e}",
                        cfg.device_index
                    );
                    eprintln!("Hint: use --list-devices to see available devices, and try a more conservative resolution like --w 640 --h 480.");
                    std::process::exit(1);
                }
            };
            FrameSource::Webcam(cam)
        }
        "ffmpeg" => {
            println!(
                "Using backend=ffmpeg (avfoundation), device index {}, {}x{}",
                cfg.device_index, cfg.width, cfg.height
            );
            let dev = match FfmpegDevice::new(cfg.device_index, cfg.width, cfg.height) {
                Ok(dev) => dev,
                Err(e) => {
                    eprintln!(
                        "Failed to open ffmpeg avfoundation device (index {}): {e}",
                        cfg.device_index
                    );
                    eprintln!("Hint: ensure ffmpeg is installed and the device index matches ffmpeg's avfoundation listing (see --list-devices).");
                    std::process::exit(1);
                }
            };
            FrameSource::Ffmpeg(dev)
        }
        other => {
            eprintln!("Unknown backend: {other}. Use 'nokhwa' or 'ffmpeg'.");
            std::process::exit(1);
        }
    };

    let mut tex_w: i32 = cfg.width as i32;
    let mut tex_h: i32 = cfg.height as i32;
    let mut host_tex = unsafe { make_host_texture(&gl, tex_w, tex_h) };

    let t0 = Instant::now();
    let mut frame_index: u64 = 0;

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

                if let Some((fw, fh, bytes)) = source.poll_rgba() {
                    let w = fw as i32;
                    let h = fh as i32;

                    if w != tex_w || h != tex_h {
                        unsafe { gl.delete_texture(host_tex) };
                        tex_w = w;
                        tex_h = h;
                        host_tex = unsafe { make_host_texture(&gl, tex_w, tex_h) };
                    }

                    unsafe {
                        upload_rgba_to_texture(&gl, host_tex, tex_w, tex_h, &bytes);
                    }
                }

                props.texture_inputs.insert(tex_in, host_tex);

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

                unsafe {
                    let outs =
                        rt::execute_plan_outputs(&gl, &g, &plan, &mut state, &props, frame_ctx)
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
