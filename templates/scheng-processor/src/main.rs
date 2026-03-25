//! scheng-processor
//!
//! Webcam → proc-amp shader → preview + Syphon output.
//!
//! Controls via MIDI CC:
//!   CC1  = brightness  (-1 → +1)
//!   CC2  = contrast    (0 → 3)
//!   CC3  = saturation  (0 → 3)
//!   CC4  = hue         (-180 → +180)
//!
//! # Run
//!
//! ```bash
//! # List cameras
//! cargo run --release -- --list-cameras
//!
//! # Use default camera (index 0)
//! cargo run --release
//!
//! # Use specific camera
//! cargo run --release -- --webcam 1
//!
//! # Custom resolution
//! cargo run --release -- --width 1920 --height 1080
//! ```

use std::{collections::HashMap, sync::{Arc, Mutex}, time::Instant};

use scheng_graph::{Graph, NodeId, NodeKind};
use scheng_hotreload::watcher::AssetWatcher;
use scheng_input_webcam::Webcam;
use scheng_param_store::{ParamStore, ParamSchema, schema::ParamDef};
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
use midir::{MidiInput as MidirInput, Ignore};

#[cfg(target_os = "macos")]
use scheng_output_syphon::SyphonSink;

// ── Constants ─────────────────────────────────────────────────────────────────

const TARGET_FPS:    u32 = 30;
const DEFAULT_WIDTH: u32 = 1280;
const DEFAULT_HEIGHT: u32 = 720;

fn frame_budget() -> std::time::Duration {
    std::time::Duration::from_nanos(1_000_000_000 / TARGET_FPS as u64)
}

// ── Args ──────────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct Args {
    width:        u32,
    height:       u32,
    msaa:         u32,
    webcam_index: usize,
    list_cameras: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self { width: DEFAULT_WIDTH, height: DEFAULT_HEIGHT, msaa: 1, webcam_index: 0, list_cameras: false }
    }
}

fn parse_args() -> Args {
    let raw: Vec<String> = std::env::args().collect();
    let mut a = Args::default();
    let mut i = 1;
    while i < raw.len() {
        match raw[i].as_str() {
            "--width"         => { i += 1; a.width        = raw[i].parse().unwrap_or(DEFAULT_WIDTH); }
            "--height"        => { i += 1; a.height       = raw[i].parse().unwrap_or(DEFAULT_HEIGHT); }
            "--msaa"          => { i += 1; a.msaa         = raw[i].parse().unwrap_or(1); }
            "--webcam"        => { i += 1; a.webcam_index = raw[i].parse().unwrap_or(0); }
            "--list-cameras"  => { a.list_cameras = true; }
            other             => log::warn!("Unknown arg: {other}"),
        }
        i += 1;
    }
    a
}

// ── ParamStore ────────────────────────────────────────────────────────────────

fn make_def(name: &str, min: f32, max: f32, default: f32, cc: Option<u8>) -> ParamDef {
    ParamDef {
        name: name.into(), ty: "float".into(),
        min, max, default, smooth: 0.05,
        midi_cc: cc, midi_channel: None,
        osc_addr: Some(format!("/scheng/{name}")),
        node_label: None, description: None,
    }
}

fn build_param_store() -> ParamStore {
    ParamStore::new(ParamSchema {
        version: 1,
        params: vec![
            make_def("u_threshold", 0.0, 1.0, 0.5, Some(1)),
            make_def("u_mix",       0.0, 1.0, 1.0, Some(2)),
        ],
    })
}

// ── Preview sink ──────────────────────────────────────────────────────────────

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
            label: Some("blit"), source: wgpu::ShaderSource::Wgsl(BLIT_WGSL.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit_layout"), bind_group_layouts: &[&bgl], push_constant_ranges: &[],
        });
        self.pipeline = Some(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit_pipeline"), layout: Some(&layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"),
                compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: self.config.format,
                    blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: Default::default(), depth_stencil: None,
            multisample: Default::default(), multiview: None, cache: None,
        }));
        self.sampler = Some(device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blit_sampler"),
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        }));
    }
}

impl OutputSink for PreviewSink {
    fn present(&mut self, _id: scheng_graph::NodeId,
        target: &scheng_runtime_wgpu::RenderTarget, _ctx: &FrameCtx,
        device: &wgpu::Device, queue: &wgpu::Queue) {
        self.configure(device);
        let frame = match self.surface.get_current_texture() { Ok(f) => f, Err(_) => return };
        let view = frame.texture.create_view(&Default::default());
        let pipeline = self.pipeline.as_ref().unwrap();
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit_bg"), layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&target.sample_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(self.sampler.as_ref().unwrap()) },
            ],
        });
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view, resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit(std::iter::once(enc.finish()));
        frame.present();
    }
}

const BLIT_WGSL: &str = r#"
struct VertOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@builtin(vertex_index) vi: u32) -> VertOut {
    var pos = array<vec2<f32>,3>(vec2(-1.,-3.),vec2(3.,1.),vec2(-1.,1.));
    var uv  = array<vec2<f32>,3>(vec2(0.,2.),vec2(2.,0.),vec2(0.,0.));
    return VertOut(vec4<f32>(pos[vi],0.,1.), uv[vi]);
}
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@fragment fn fs_main(v: VertOut) -> @location(0) vec4<f32> {
    return textureSample(t, s, vec2(v.uv.x, 1.0 - v.uv.y));
}
"#;

// ── Graph ─────────────────────────────────────────────────────────────────────

struct ProcessorGraph {
    graph:    Graph,
    node_src: NodeId,  // webcam passthrough
    node_fx:  NodeId,  // proc-amp shader
    node_out: NodeId,  // pixels out
}

impl ProcessorGraph {
    fn new() -> Self {
        let mut g = Graph::new();
        let node_src = g.add_node(NodeKind::ShaderSource);
        let node_fx  = g.add_node(NodeKind::ShaderPass);
        let node_out = g.add_node(NodeKind::PixelsOut);
        g.connect_named(node_src, "out", node_fx,  "in").unwrap();
        g.connect_named(node_fx,  "out", node_out, "in").unwrap();
        Self { graph: g, node_src, node_fx, node_out }
    }
}

// ── Instrument ────────────────────────────────────────────────────────────────

struct Processor {
    args:        Args,
    runtime:     Option<WgpuRuntime>,
    preview:     Option<PreviewSink>,
    window:      Option<Arc<Window>>,
    watcher:     Option<AssetWatcher>,
    pg:          ProcessorGraph,
    param_store: Arc<Mutex<ParamStore>>,
    midi:        Option<midir::MidiInputConnection<()>>,
    webcam:      Option<Webcam>,
    solarize:    Option<String>,
    frame:       u64,
    start:       Instant,
    last:        Instant,

    #[cfg(target_os = "macos")]
    syphon_out: Option<SyphonSink>,
}

impl Processor {
    fn new() -> Self {
        let args = parse_args();

        // List cameras and exit
        if args.list_cameras {
            let cams = Webcam::list_cameras();
            println!("Available cameras:");
            for (i, name) in cams.iter().enumerate() {
                println!("  [{i}] {name}");
            }
            std::process::exit(0);
        }

        Self {
            args,
            runtime:     None,
            preview:     None,
            window:      None,
            watcher:     None,
            pg:          ProcessorGraph::new(),
            param_store: Arc::new(Mutex::new(build_param_store())),
            midi:        None,
            webcam:      None,
            solarize:    None,
            frame:       0,
            start:       Instant::now(),
            last:        Instant::now(),
            #[cfg(target_os = "macos")]
            syphon_out: None,
        }
    }

    fn tick(&mut self) {
        if Instant::now().duration_since(self.last) < frame_budget() { return; }
        self.last = Instant::now();

        // Hot-reload
        if let Some(ref mut w) = self.watcher {
            if !w.drain().is_empty() {
                self.solarize = std::fs::read_to_string("assets/shaders/solarize.frag").ok();
                log::info!("Hot-reloaded solarize.frag");
            }
        }

        // Poll webcam
        if let (Some(ref mut cam), Some(ref r)) = (&mut self.webcam, &self.runtime) {
            cam.poll(&r.ctx.queue);
        }

        let (Some(ref mut runtime), Some(ref mut preview)) =
            (&mut self.runtime, &mut self.preview) else { return };

        // Step param smoother and read values
        let uniforms = {
            let mut store = self.param_store.lock().unwrap();
            store.step_frame();
            store.all_values().clone()
        };

        let time = self.start.elapsed().as_secs_f32();
        let ctx  = FrameCtx {
            width: self.args.width, height: self.args.height,
            time, frame: self.frame, sample_count: self.args.msaa,
        };

        let mut configs: HashMap<NodeId, NodeConfig> = HashMap::new();

        // Source node — inject webcam texture with Y-flip
        let mut cfg_src = NodeConfig::default();
        if let Some(ref cam) = self.webcam {
            cfg_src.input_textures[0] = cam.texture_arc();
            cfg_src.frag_shader = Some(
                "void main() { fragColor = texture(iChannel0, vec2(v_uv.x, 1.0 - v_uv.y)); }".into()
            );
        }
        configs.insert(self.pg.node_src, cfg_src);

        // FX node — proc-amp with all uniform values
        let mut cfg_fx = NodeConfig::default();
        cfg_fx.frag_shader = self.solarize.clone();
        cfg_fx.uniforms = uniforms.clone();
        configs.insert(self.pg.node_fx, cfg_fx);

        configs.insert(self.pg.node_out, NodeConfig::default());

        let plan = match self.pg.graph.compile() {
            Ok(p)  => p,
            Err(e) => { log::error!("Graph: {e}"); return; }
        };

        if let Err(e) = runtime.execute_frame(&self.pg.graph, &plan, &configs, &ctx, preview) {
            log::error!("execute_frame: {e}");
        }

        #[cfg(target_os = "macos")]
        if let Some(ref mut so) = self.syphon_out {
            if let Err(e) = runtime.execute_frame(&self.pg.graph, &plan, &configs, &ctx, so) {
                log::error!("execute_frame syphon: {e}");
            }
        }

        if self.frame % TARGET_FPS as u64 == 0 {
            let thresh = uniforms.get("u_threshold").copied().unwrap_or(0.5);
            let mix    = uniforms.get("u_mix").copied().unwrap_or(1.0);
            log::info!("t={:.1}s | threshold={thresh:.2} mix={mix:.2}", time);
        }

        self.frame += 1;
    }
}

impl ApplicationHandler for Processor {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let win = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("scheng-processor")
                    .with_inner_size(winit::dpi::LogicalSize::new(self.args.width, self.args.height))
            ).unwrap()
        );

        let runtime = WgpuRuntime::new(self.args.width, self.args.height)
            .expect("wgpu runtime");

        // Open webcam FIRST to get native resolution
        // Webcam
        let cams = Webcam::list_cameras();
        if cams.is_empty() {
            log::warn!("No cameras found");
        } else {
            for (i, name) in cams.iter().enumerate() {
                log::info!("Camera [{i}]: {name}");
            }
            // Request 1280x720 — FaceTime HD native resolution
        // The webcam SDK allocates the texture at the actual delivered size
        self.webcam = Webcam::open(
                self.args.webcam_index as u32,
                1280, 720,
                &runtime.ctx.device, &runtime.ctx.queue,
            )
            .map(|c| { log::info!("Webcam [{}]: {}×{} ready", self.args.webcam_index as u32, c.width(), c.height()); c })
            .map_err(|e| log::warn!("Webcam: {e}")).ok();
        }

        // Create surface at webcam's native resolution
        let surface = runtime.ctx.instance.create_surface(win.clone()).expect("surface");
        let caps    = surface.get_capabilities(&runtime.ctx.adapter);
        let format  = caps.formats.iter().find(|f| f.is_srgb()).copied().unwrap_or(caps.formats[0]);
        let config  = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT, format,
            width: self.args.width, height: self.args.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0], view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&runtime.ctx.device, &config);

        // MIDI
        let midi_store = Arc::clone(&self.param_store);
        self.midi = (|| -> Option<midir::MidiInputConnection<()>> {
            let mut midi_in = MidirInput::new("scheng-processor").ok()?;
            midi_in.ignore(Ignore::None);
            let ports = midi_in.ports();
            if ports.is_empty() { log::warn!("MIDI: no ports found"); return None; }
            for p in &ports {
                if let Ok(name) = midi_in.port_name(p) { log::info!("MIDI port: '{name}'"); }
            }
            let port = ports.into_iter().next()?;
            let name = midi_in.port_name(&port).unwrap_or_default();
            let conn = midi_in.connect(&port, "scheng-processor-in", move |_ts, msg, _| {
                if msg.len() == 3 && (msg[0] & 0xF0) == 0xB0 {
                    let cc = msg[1]; let val = msg[2];
                    log::info!("[MIDI] CC{cc} = {val}");
                    if let Ok(mut s) = midi_store.lock() {
                        let _ = s.set_by_midi_cc(cc, val);
                    }
                }
            }, ()).ok()?;
            log::info!("MIDI connected: '{name}'");
            Some(conn)
        })();

        // Reconfigure surface to webcam native resolution
        if let Some(ref pv_config) = None::<()> { let _ = pv_config; } // placeholder
        // (surface reconfigured after preview is assigned below)

        // Syphon output
        #[cfg(target_os = "macos")]
        {
            self.syphon_out = SyphonSink::new("scheng-processor")
                .map(|s| { log::info!("Syphon out: 'scheng-processor' ready"); s })
                .map_err(|e| log::warn!("Syphon out: {e}")).ok();
        }

        // Load shader
        self.solarize = std::fs::read_to_string("assets/shaders/solarize.frag").ok();
        if self.solarize.is_none() { log::warn!("solarize.frag not found"); }

        self.watcher = AssetWatcher::new("assets")
            .map_err(|e| log::warn!("Watcher: {e}")).ok();

        log::info!("{}×{} @ {}fps | MIDI CC1=threshold CC2=mix",
            self.args.width, self.args.height, TARGET_FPS);

        self.preview = Some(PreviewSink::new(surface, config));
        self.runtime = Some(runtime);
        self.window  = Some(win);
        self.start   = Instant::now();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event: KeyEvent {
                physical_key: PhysicalKey::Code(KeyCode::Escape),
                state: ElementState::Pressed, ..
            }, .. } => event_loop.exit(),
            WindowEvent::Resized(sz) => {
                if sz.width > 0 && sz.height > 0 {
                    self.args.width  = sz.width;
                    self.args.height = sz.height;
                    if let (Some(ref rt), Some(ref mut pv)) = (&self.runtime, &mut self.preview) {
                        pv.config.width  = sz.width;
                        pv.config.height = sz.height;
                        pv.surface.configure(&rt.ctx.device, &pv.config);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(ref win) = self.window { win.request_redraw(); }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) { self.tick(); }
}

fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();
    EventLoop::new().unwrap().run_app(&mut Processor::new()).unwrap();
}
