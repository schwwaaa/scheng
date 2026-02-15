use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::time::{Duration, Instant};

use glow::HasContext;

use glutin::config::{ConfigTemplateBuilder, GlConfig};
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
use winit::event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

use scheng_graph as graph;
use scheng_runtime::{BankSet, MatrixMixParams, MatrixPreset};
use scheng_runtime_glow as rt;

struct PresentBlitSink {
    w: i32,
    h: i32,
}

impl rt::OutputSink for PresentBlitSink {
    fn consume(&mut self, gl: &glow::Context, out: &rt::ExecOutput) {
        unsafe {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(out.fbo));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
            gl.blit_framebuffer(
                0,
                0,
                out.width,
                out.height,
                0,
                0,
                self.w,
                self.h,
                glow::COLOR_BUFFER_BIT,
                glow::LINEAR,
            );
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
        }
    }
}



mod banks;
mod quant;
mod queue;
mod transition;

use quant::Quantizer;
use queue::QueuedStep;
use transition::PresetTransition;

// ---------------- shaders ----------------

const FRAG_R: &str = r#"#version 330 core
uniform vec2 uResolution;
uniform float uTime;
out vec4 FragColor;
void main(){
    vec2 uv = gl_FragCoord.xy / uResolution.xy;
    float v = 0.5 + 0.5 * sin(uv.x * 10.0 + uTime);
    FragColor = vec4(v, 0.0, 0.0, 1.0);
}
"#;

const FRAG_G: &str = r#"#version 330 core
uniform vec2 uResolution;
uniform float uTime;
out vec4 FragColor;
void main(){
    vec2 uv = gl_FragCoord.xy / uResolution.xy;
    float v = 0.5 + 0.5 * sin(uv.y * 12.0 - uTime);
    FragColor = vec4(0.0, v, 0.0, 1.0);
}
"#;

const FRAG_B: &str = r#"#version 330 core
uniform vec2 uResolution;
uniform float uTime;
out vec4 FragColor;
void main(){
    vec2 uv = gl_FragCoord.xy / uResolution.xy;
    float v = 0.5 + 0.5 * cos((uv.x + uv.y) * 8.0 + uTime);
    FragColor = vec4(0.0, 0.0, v, 1.0);
}
"#;

const FRAG_W: &str = r#"#version 330 core
uniform vec2 uResolution;
uniform float uTime;
out vec4 FragColor;
void main(){
    vec2 uv = gl_FragCoord.xy / uResolution.xy;
    float v = 0.5 + 0.5 * sin((uv.x - uv.y) * 9.0 - uTime);
    FragColor = vec4(v, v, v, 1.0);
}
"#;

// C4i: the "example patterns" are extracted into small local modules.

fn main() {
    // ---------------- Window + GL ----------------
    let event_loop = EventLoop::new();

    let window_builder = WindowBuilder::new()
        .with_title("scheng matrix mix4 (C4g: queued + quantized scenes)")
        .with_inner_size(PhysicalSize::new(960, 540));

    let template = ConfigTemplateBuilder::new();

    let display_builder = DisplayBuilder::new().with_window_builder(Some(window_builder));
    let (window, gl_config) = display_builder
        .build(&event_loop, template, |configs| {
            configs
                .reduce(|a, b| {
                    if b.num_samples() > a.num_samples() {
                        b
                    } else {
                        a
                    }
                })
                .unwrap()
        })
        .unwrap();

    let window = window.expect("window");
    let gl_display = gl_config.display();

    let context_attrs = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .build(Some(window.raw_window_handle()));

    let not_current: NotCurrentContext = unsafe {
        gl_display
            .create_context(&gl_config, &context_attrs)
            .unwrap()
    };

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

    // ---------------- Graph ----------------
    let mut g = graph::Graph::new();

    let p0 = g.add_node(graph::NodeKind::ShaderPass);
    let p1 = g.add_node(graph::NodeKind::ShaderPass);
    let p2 = g.add_node(graph::NodeKind::ShaderPass);
    let p3 = g.add_node(graph::NodeKind::ShaderPass);

    let mix = g.add_node(graph::NodeKind::MatrixMix4);
    let out = g.add_node(graph::NodeKind::PixelsOut);

    // PortId order semantics: in0..in3
    g.connect_named(p0, "out", mix, "in0").unwrap();
    g.connect_named(p1, "out", mix, "in1").unwrap();
    g.connect_named(p2, "out", mix, "in2").unwrap();
    g.connect_named(p3, "out", mix, "in3").unwrap();
    g.connect_named(mix, "out", out, "in").unwrap();

    let plan = g.compile().unwrap();

    // ---------------- Runtime ----------------
    let mut props = rt::NodeProps::default();

    props.shader_sources.insert(
        p0,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: FRAG_R.to_string(),
            origin: Some("graph_matrix_mix4:p0".to_string()),
        },
    );
    props.shader_sources.insert(
        p1,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: FRAG_G.to_string(),
            origin: Some("graph_matrix_mix4:p1".to_string()),
        },
    );
    props.shader_sources.insert(
        p2,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: FRAG_B.to_string(),
            origin: Some("graph_matrix_mix4:p2".to_string()),
        },
    );
    props.shader_sources.insert(
        p3,
        rt::ShaderSource {
            vert: rt::FULLSCREEN_VERT.to_string(),
            frag: FRAG_W.to_string(),
            origin: Some("graph_matrix_mix4:p3".to_string()),
        },
    );

    let mut state = unsafe { rt::RuntimeState::new(&gl) }.unwrap();
    let start = Instant::now();

    // C4e: smooth transitions (still used)
    let transition_duration = Duration::from_secs(1);
    let mut current_preset = MatrixPreset::Quad;
    let mut transition: Option<PresetTransition> = None;

    // Animation mode (improv). When true, we ignore banks/queue.
    let mut animate = true;

    // C4g: quantized queued switching (default 500ms quantum)
    let quantum = Duration::from_millis(500);
    let quant = Quantizer::new(quantum, start);
    let mut last_slot: u64 = quant.slot();

    // "Armed" means: at each quant boundary, pop next queued scene and transition to it.
    let mut armed = false;
    let mut queue: VecDeque<QueuedStep> = VecDeque::new();

    // Banks: either JSON or built-in (JSON is loaded via scheng-runtime standard helpers)
    let mut banks: BankSet = BankSet::builtin_matrix_banks();
    let mut bank_idx: usize = 0;

    if let Some(path) = banks::parse_args_banks_path() {
        match BankSet::from_json_path(&path) {
            Ok(b) => {
                eprintln!("[banks] loaded from {}", path.display());
                banks = b;
            }
            Err(e) => {
                eprintln!("[banks] failed to load {}: {}", path.display(), e);
                eprintln!("[banks] falling back to built-in");
            }
        }
    } else {
        eprintln!("[banks] built-in (use --banks <file.json> to load)");
    }

    // Print initial bank
    // Print initial bank
    banks::print_bank(&banks, bank_idx);

    // ---------------- Event loop ----------------
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    unsafe { state.destroy(&gl) };
                    *control_flow = ControlFlow::Exit;
                }

                WindowEvent::Resized(new_size) => {
                    let w = new_size.width.max(1);
                    let h = new_size.height.max(1);
                    gl_surface.resize(
                        &gl_context,
                        NonZeroU32::new(w).unwrap(),
                        NonZeroU32::new(h).unwrap(),
                    );
                }

                WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            state: ElementState::Pressed,
                            virtual_keycode: Some(key),
                            ..
                        },
                    ..
                } => {
                    // Bank navigation (works regardless of mode)
                    match key {
                        VirtualKeyCode::LBracket => {
                            let bank_count = banks.banks.len();
                            bank_idx = if bank_idx == 0 {
                                bank_count - 1
                            } else {
                                bank_idx - 1
                            };

                            banks::print_bank(&banks, bank_idx);
                            return;
                        }
                        VirtualKeyCode::RBracket => {
                            let bank_count = banks.banks.len();
                            bank_idx = (bank_idx + 1) % bank_count;

                            banks::print_bank(&banks, bank_idx);
                            return;
                        }
                        VirtualKeyCode::Return => {
                            // In C4g: Enter is the "arm" toggle, plus print status.
                            armed = !armed;
                            eprintln!(
                                "[queue] armed={armed} quantum={}ms len={}",
                                quantum.as_millis(),
                                queue.len()
                            );

                            banks::print_bank(&banks, bank_idx);
                            return;
                        }
                        VirtualKeyCode::Back => {
                            queue.clear();
                            eprintln!("[queue] cleared");
                            return;
                        }
                        _ => {}
                    }

                    // Animation toggle
                    if key == VirtualKeyCode::Space {
                        animate = !animate;
                        if animate {
                            armed = false;
                            queue.clear();
                            transition = None;
                            eprintln!("[animate] true (queue/banks disabled; queue cleared)");
                        } else {
                            eprintln!("[animate] false (queue/banks enabled)");
                        }
                        return;
                    }

                    // Scene enqueue (1–9)
                    let scene_index_0: Option<usize> = match key {
                        VirtualKeyCode::Key1 => Some(0),
                        VirtualKeyCode::Key2 => Some(1),
                        VirtualKeyCode::Key3 => Some(2),
                        VirtualKeyCode::Key4 => Some(3),
                        VirtualKeyCode::Key5 => Some(4),
                        VirtualKeyCode::Key6 => Some(5),
                        VirtualKeyCode::Key7 => Some(6),
                        VirtualKeyCode::Key8 => Some(7),
                        VirtualKeyCode::Key9 => Some(8),
                        _ => None,
                    };

                    if let Some(i0) = scene_index_0 {
                        // Enqueue only when not animating
                        animate = false;

                        // Resolve preset from current bank
                        {
                            let bank = &banks.banks[bank_idx];
                            if i0 >= bank.scenes.len() {
                                eprintln!("[scene] {} out of range for bank {}", i0 + 1, bank.name);
                                return;
                            }
                            let s = &bank.scenes[i0];
                            queue.push_back(QueuedStep {
                                bank_idx,
                                scene_idx: i0,
                                preset: s.preset,
                            });
                            eprintln!(
                                "[queue] + {} | {} ({}): {}  (len={})",
                                bank.name,
                                i0 + 1,
                                s.name,
                                s.preset.name(),
                                queue.len()
                            );
                        }
                    }
                }

                _ => {}
            },

            Event::MainEventsCleared => window.request_redraw(),

            Event::RedrawRequested(_) => {
                let size = window.inner_size();
                let w = size.width.max(1);
                let h = size.height.max(1);

                let frame = rt::FrameCtx {
                    time: start.elapsed().as_secs_f32(),
                    width: w as i32,
                    height: h as i32,
                    frame: 0,
                };

                let t = frame.time;

                // C4g: On quant boundary, if armed and idle (not transitioning), pop next queued step.
                // This ensures determinism and avoids changing targets mid-transition.
                    if !animate
                        && quant.is_boundary(&mut last_slot)
                        && armed
                        && transition.is_none()
                    {
                            if let Some(step) = queue.pop_front() {
                                let to = step.preset;
                                if to != current_preset {
                                    transition = Some(PresetTransition::new(
                                        current_preset,
                                        to,
                                        transition_duration,
                                    ));

                                    // Print scene name if we can
                                    {
                                        let bank = &banks.banks[step.bank_idx];
                                        let scene = &bank.scenes[step.scene_idx];
                                        eprintln!(
                                            "[apply] {} | {} ({}): {} -> {} ({}ms)  (queue len={})",
                                            bank.name,
                                            step.scene_idx + 1,
                                            scene.name,
                                            current_preset.name(),
                                            to.name(),
                                            transition_duration.as_millis(),
                                            queue.len()
                                        );
                                    }
                                } else {
                                    eprintln!(
                                        "[apply] skipped (already {})",
                                        current_preset.name()
                                    );
                                }
                            }
                        }
                    
                

                // Determine weights (animation OR transition OR static preset)
                let (weights, done_transition) = if animate {
                    // Smooth 4-way orbit, normalized
                    let w0 = 0.5 + 0.5 * (t * 0.7).sin();
                    let w1 = 0.5 + 0.5 * (t * 0.9 + 1.0).sin();
                    let w2 = 0.5 + 0.5 * (t * 1.1 + 2.0).sin();
                    let w3 = 0.5 + 0.5 * (t * 1.3 + 3.0).sin();
                    let s = (w0 + w1 + w2 + w3).max(0.0001_f32);
                    ([w0 / s, w1 / s, w2 / s, w3 / s], false)
                } else if let Some(tr) = transition {
                    let (wts, done) = tr.weights();
                    (wts, done)
                } else {
                    (current_preset.params().weights, false)
                };

                // Finalize transition if needed
                if !animate && done_transition {
                    if let Some(tr) = transition {
                        current_preset = tr.to;
                        transition = None;
                        eprintln!("[preset] now {}", current_preset.name());
                    }
                }

                props.matrix_params.insert(mix, MatrixMixParams { weights });

                let mut sink = PresentBlitSink { w: size.width as i32, h: size.height as i32 };

                let exec = match unsafe {
                    rt::execute_plan_to_sink(&gl, &g, &plan, &mut state, &props, frame, &mut sink)
                } {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("execute_plan error: {e:?}");
                        return;
                    }
                };

                unsafe {
                    gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(exec.fbo));
                    gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
                    gl.blit_framebuffer(
                        0,
                        0,
                        exec.width,
                        exec.height,
                        0,
                        0,
                        w as i32,
                        h as i32,
                        glow::COLOR_BUFFER_BIT,
                        glow::NEAREST,
                    );
                    gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
                }

                gl_surface.swap_buffers(&gl_context).unwrap();
            }

            _ => {}
        }
    });
}
