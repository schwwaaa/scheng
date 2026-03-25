//! scheng-gradient
//!
//! Minimal scheng instrument. Opens a window and hot-reloads a GLSL fragment
//! shader from `assets/shaders/main.frag`. Edit the shader and save — the
//! instrument updates immediately without restarting.
//!
//! # Run
//!
//! ```bash
//! cargo run --release
//! cargo run --release -- --width 1920 --height 1080
//! cargo run --release -- --width 3840 --height 2160 --msaa 4
//! cargo run --release -- --stream rtmp://localhost:1935/live/key
//! cargo run --release -- --record output.mp4
//! ```
//!
//! # Adding I/O
//!
//! Uncomment the relevant lines in Cargo.toml and main.rs to enable:
//! - MIDI control (--midi)
//! - OSC control (--osc)
//! - Syphon output (--syphon, macOS only)
//! - NDI output (--ndi)
//! - Webcam input (--webcam 0)
//! - Video file input (--video path/to/file.mp4)
//! - RTMP/RTSP stream output (--stream url)
//! - File recording (--record path/to/output.mp4)

use std::{
    collections::HashMap,
    sync::Arc,
    time::Instant,
};

use scheng_graph::{Graph, NodeId, NodeKind};
use scheng_hotreload::watcher::AssetWatcher;
use scheng_runtime_wgpu::{
    executor::{NodeConfig, OutputSink},
    FrameCtx, WgpuRuntime,
};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

// ── Constants ─────────────────────────────────────────────────────────────────

const TARGET_FPS:    u32 = 30;
const DEFAULT_WIDTH: u32 = 1280;
const DEFAULT_HEIGHT: u32 = 720;
const SHADER_PATH:   &str = "assets/shaders/main.frag";

fn frame_budget() -> std::time::Duration {
    std::time::Duration::from_nanos(1_000_000_000 / TARGET_FPS as u64)
}

// ── Args ──────────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct Args {
    width:      u32,
    height:     u32,
    msaa:       u32,
    stream_url: Option<String>,
    record:     Option<String>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            width:      DEFAULT_WIDTH,
            height:     DEFAULT_HEIGHT,
            msaa:       1,
            stream_url: None,
            record:     None,
        }
    }
}

fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().collect();
    let mut a = Args::default();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--width"   => { i += 1; a.width      = args[i].parse().unwrap_or(DEFAULT_WIDTH); }
            "--height"  => { i += 1; a.height     = args[i].parse().unwrap_or(DEFAULT_HEIGHT); }
            "--msaa"    => { i += 1; a.msaa       = args[i].parse().unwrap_or(1); }
            "--stream"  => { i += 1; a.stream_url = Some(args[i].clone()); }
            "--record"  => { i += 1; a.record     = Some(args[i].clone()); }
            other       => log::warn!("Unknown arg: {other}"),
        }
        i += 1;
    }
    a
}

// ── Preview sink (window display) ────────────────────────────────────────────

struct PreviewSink {
    surface:  wgpu::Surface<'static>,
    config:   wgpu::SurfaceConfiguration,
    pipeline: Option<wgpu::RenderPipeline>,
    sampler:  Option<wgpu::Sampler>,
}

impl PreviewSink {
    fn new(surface: wgpu::Surface<'static>, config: wgpu::SurfaceConfiguration) -> Self {
        Self { surface, config, pipeline: None, sampler: None }
    }

    fn configure(&mut self, device: &wgpu::Device) {
        if self.pipeline.is_some() { return; }

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("preview_blit"),
            source: wgpu::ShaderSource::Wgsl(BLIT_SHADER.into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("preview_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type:    wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled:   false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:                Some("preview_layout"),
            bind_group_layouts:   &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("preview_pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module:               &shader,
                entry_point:          Some("vs_main"),
                compilation_options:  wgpu::PipelineCompilationOptions::default(),
                buffers:              &[],
            },
            fragment: Some(wgpu::FragmentState {
                module:              &shader,
                entry_point:         Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format:     self.config.format,
                    blend:      None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive:     wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            multiview:     None,
            cache:         None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label:      Some("preview_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        self.pipeline = Some(pipeline);
        self.sampler  = Some(sampler);
    }
}

impl OutputSink for PreviewSink {
    fn present(
        &mut self,
        _node_id: scheng_graph::NodeId,
        target:   &scheng_runtime_wgpu::RenderTarget,
        _ctx:     &FrameCtx,
        device:   &wgpu::Device,
        queue:    &wgpu::Queue,
    ) {
        self.configure(device);

        let frame = match self.surface.get_current_texture() {
            Ok(f)  => f,
            Err(_) => return,
        };

        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let src_view = &target.sample_view;

        let pipeline = self.pipeline.as_ref().unwrap();
        let sampler  = self.sampler.as_ref().unwrap();

        // Build bind group referencing the rendered texture
        let bgl = pipeline.get_bind_group_layout(0);
        let bg  = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("preview_bg"),
            layout:  &bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(src_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        });

        let mut encoder = device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("preview_enc") }
        );
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("preview_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view:           &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load:  wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes:         None,
                occlusion_query_set:      None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}

const BLIT_SHADER: &str = r#"
@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>( 3.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );
    return vec4<f32>(pos[vi], 0.0, 1.0);
}

@group(0) @binding(0) var t_frame: texture_2d<f32>;
@group(0) @binding(1) var s_frame: sampler;

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(t_frame));
    let uv   = pos.xy / dims;
    return textureSample(t_frame, s_frame, vec2<f32>(uv.x, 1.0 - uv.y));
}
"#;

// ── Graph setup ───────────────────────────────────────────────────────────────

fn build_graph() -> (Graph, NodeId, NodeId) {
    let mut g = Graph::new();
    let src = g.add_node(NodeKind::ShaderSource);
    let out = g.add_node(NodeKind::PixelsOut);
    g.connect_named(src, "out", out, "in").unwrap();
    (g, src, out)
}

// ── Instrument ────────────────────────────────────────────────────────────────

struct Instrument {
    args:      Args,
    runtime:   Option<WgpuRuntime>,
    preview:   Option<PreviewSink>,
    window:    Option<Arc<Window>>,
    watcher:   Option<AssetWatcher>,
    graph:     Graph,
    src_node:  NodeId,
    out_node:  NodeId,
    frame:     u64,
    start:     Instant,
    last_tick: Instant,
    shader:    Option<String>,
}

impl Instrument {
    fn new() -> Self {
        let (graph, src_node, out_node) = build_graph();
        Self {
            args:      parse_args(),
            runtime:   None,
            preview:   None,
            window:    None,
            watcher:   None,
            graph,
            src_node,
            out_node,
            frame:     0,
            start:     Instant::now(),
            last_tick: Instant::now(),
            shader:    None,
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_tick) < frame_budget() { return; }
        self.last_tick = now;

        // Hot-reload check
        if let Some(ref mut w) = self.watcher {
            if !w.drain().is_empty() {
                match std::fs::read_to_string(SHADER_PATH) {
                    Ok(new_src) => {
                        log::info!("Hot-reloaded {SHADER_PATH}");
                        self.shader = Some(new_src);
                        if let Some(ref mut rt) = self.runtime {
                            rt.pipelines.clear();
                        }
                    }
                    Err(e) => log::warn!("Hot-reload read failed: {e}"),
                }
            }
        }

        let (Some(ref mut runtime), Some(ref mut preview)) =
            (&mut self.runtime, &mut self.preview) else { return };

        let time = self.start.elapsed().as_secs_f32();
        let ctx  = FrameCtx {
            width:        self.args.width,
            height:       self.args.height,
            time,
            frame:        self.frame,
            sample_count: self.args.msaa,
        };

        // Build per-frame node configs
        let mut configs = HashMap::new();
        let mut src_cfg = NodeConfig::default();
        src_cfg.frag_shader = self.shader.clone();
        configs.insert(self.src_node, src_cfg);
        configs.insert(self.out_node, NodeConfig::default());

        let plan = match self.graph.compile() {
            Ok(p)  => p,
            Err(e) => { log::error!("Graph compile: {e}"); return; }
        };

        if let Err(e) = runtime.execute_frame(&self.graph, &plan, &configs, &ctx, preview) {
            log::error!("execute_frame: {e}");
        }

        if self.frame % (TARGET_FPS as u64 * 10) == 0 {
            log::info!("Frame {} | t={:.1}s", self.frame, time);
        }

        self.frame += 1;

    }
}

impl ApplicationHandler for Instrument {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let win_attrs = Window::default_attributes()
            .with_title("scheng-gradient")
            .with_inner_size(winit::dpi::LogicalSize::new(self.args.width, self.args.height));

        let win = Arc::new(event_loop.create_window(win_attrs).unwrap());
        let runtime = WgpuRuntime::new(self.args.width, self.args.height)
            .expect("Failed to create wgpu runtime");

        // Create surface from the runtime's instance — same device, no conflict
        let surface = runtime.ctx.instance
            .create_surface(win.clone())
            .expect("surface");

        let caps   = surface.get_capabilities(&runtime.ctx.adapter);
        let format = caps.formats.iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage:        wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width:        self.args.width,
            height:       self.args.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode:   caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&runtime.ctx.device, &config);

        // Load shader
        self.shader = std::fs::read_to_string(SHADER_PATH).ok();
        if self.shader.is_none() {
            log::warn!("Shader {SHADER_PATH} not found — using built-in gradient");
        }

        // Hot-reload watcher
        self.watcher = AssetWatcher::new("assets")
            .map_err(|e| log::warn!("Hot-reload: {e}"))
            .ok();

        log::info!(
            "{}×{} @ {}fps | MSAA {}x | edit {SHADER_PATH} to hot-reload",
            self.args.width, self.args.height, TARGET_FPS, self.args.msaa
        );

        self.preview = Some(PreviewSink::new(surface, config));
        self.runtime = Some(runtime);
        self.window  = Some(win);
        self.start   = Instant::now();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput {
                event: KeyEvent {
                    physical_key: PhysicalKey::Code(KeyCode::Escape),
                    state: ElementState::Pressed, ..
                }, ..
            } => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(ref mut rt) = self.runtime {
                    if let Some(ref mut pv) = self.preview {
                        pv.config.width  = size.width;
                        pv.config.height = size.height;
                        pv.surface.configure(&rt.ctx.device, &pv.config);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(ref win) = self.window {
                    win.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        self.tick();
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    let event_loop = EventLoop::new().unwrap();
    event_loop.run_app(&mut Instrument::new()).unwrap();
}
