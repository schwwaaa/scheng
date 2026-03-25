//! scheng-mixer
//!
//! Two-channel video mixer with Syphon A/B inputs, MIDI T-bar crossfade,
//! and Syphon output. Inspired by analog Panasonic/Videonics mixer signal flow.
//!
//! # Signal chain
//!
//! ```text
//! Syphon "A" ─→ node_a (passthrough) ─┐
//!                                       ├─→ node_mix (crossfade) ─→ Syphon "scheng-mixer" + preview
//! Syphon "B" ─→ node_b (passthrough) ─┘
//! ```
//!
//! # Run
//!
//! ```bash
//! # List available Syphon sources, then connect:
//! cargo run --release -- --syphon-a "Resolume Arena" --syphon-b "OBS"
//!
//! # Without Syphon inputs (gradient placeholders):
//! cargo run --release
//! ```
//!
//! # MIDI
//!
//! CC1  = T-bar (0→127 maps to A→B)
//! CC7  = Master output level
//!
//! # OSC
//!
//! /scheng/tbar       0.0–1.0
//! /scheng/level      0.0–1.0

use std::{collections::HashMap, sync::{Arc, Mutex}, time::Instant};

use scheng_graph::{Graph, NodeId, NodeKind};
use scheng_hotreload::watcher::AssetWatcher;
use scheng_param_store::{ParamStore, ParamSchema, schema::ParamDef};
use midir::{MidiInput as MidirInput, Ignore};
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

#[cfg(target_os = "macos")]
use scheng_input_syphon::SyphonReceiver;
#[cfg(target_os = "macos")]
use scheng_output_syphon::SyphonSink;

// ── Constants ─────────────────────────────────────────────────────────────────

const TARGET_FPS:     u32 = 30;
const DEFAULT_WIDTH:  u32 = 1280;
const DEFAULT_HEIGHT: u32 = 720;

fn frame_budget() -> std::time::Duration {
    std::time::Duration::from_nanos(1_000_000_000 / TARGET_FPS as u64)
}

// ── Args ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct Args {
    width:     u32,
    height:    u32,
    msaa:      u32,
    syphon_a:  Option<String>,
    syphon_b:  Option<String>,
}

fn parse_args() -> Args {
    let raw: Vec<String> = std::env::args().collect();
    let mut a = Args { width: DEFAULT_WIDTH, height: DEFAULT_HEIGHT, msaa: 1, ..Default::default() };
    let mut i = 1;
    while i < raw.len() {
        match raw[i].as_str() {
            "--width"     => { i += 1; a.width    = raw[i].parse().unwrap_or(DEFAULT_WIDTH); }
            "--height"    => { i += 1; a.height   = raw[i].parse().unwrap_or(DEFAULT_HEIGHT); }
            "--msaa"      => { i += 1; a.msaa     = raw[i].parse().unwrap_or(1); }
            "--syphon-a"  => { i += 1; a.syphon_a = Some(raw[i].clone()); }
            "--syphon-b"  => { i += 1; a.syphon_b = Some(raw[i].clone()); }
            other         => log::warn!("Unknown arg: {other}"),
        }
        i += 1;
    }
    a
}

// ── Mixer state ───────────────────────────────────────────────────────────────

// ── ParamStore ───────────────────────────────────────────────────────────────

fn make_param_def(name: &str, min: f32, max: f32, default: f32, midi_cc: Option<u8>, smooth: f32) -> ParamDef {
    ParamDef {
        name: name.into(), ty: "float".into(),
        min, max, default, smooth,
        midi_cc, midi_channel: None, osc_addr: None,
        node_label: None, description: None,
    }
}

fn build_param_store() -> ParamStore {
    let schema = ParamSchema {
        version: 1,
        params: vec![
            make_param_def("u_tbar",    0.0, 1.0, 0.0, Some(1), 0.05),
            make_param_def("u_level",   0.0, 1.0, 1.0, Some(7), 0.05),
            make_param_def("u_softness",0.0, 0.5, 0.0, None,    0.0),
        ],
    };
    ParamStore::new(schema)
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
            label:  Some("preview_blit"),
            source: wgpu::ShaderSource::Wgsl(BLIT_WGSL.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("preview_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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
            label: Some("preview_layout"), bind_group_layouts: &[&bgl], push_constant_ranges: &[],
        });
        self.pipeline = Some(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("preview_pipeline"), layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader, entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(), buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader, entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.config.format, blend: None, write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None, multisample: wgpu::MultisampleState::default(),
            multiview: None, cache: None,
        }));
        self.sampler = Some(device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("preview_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        }));
    }
}

impl OutputSink for PreviewSink {
    fn present(
        &mut self, _node_id: scheng_graph::NodeId,
        target: &scheng_runtime_wgpu::RenderTarget,
        _ctx: &FrameCtx, device: &wgpu::Device, queue: &wgpu::Queue,
    ) {
        self.configure(device);
        let frame = match self.surface.get_current_texture() { Ok(f) => f, Err(_) => return };
        let view     = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let src_view = &target.sample_view;
        let pipeline = self.pipeline.as_ref().unwrap();
        let sampler  = self.sampler.as_ref().unwrap();
        let bgl = pipeline.get_bind_group_layout(0);
        let bg  = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("preview_bg"), layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(src_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("preview_enc") });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("preview_pass"),
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
@vertex fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    var pos = array<vec2<f32>,3>(vec2(-1.,-3.), vec2(3.,1.), vec2(-1.,1.));
    return vec4<f32>(pos[vi], 0., 1.);
}
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@fragment fn fs_main(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let d = vec2<f32>(textureDimensions(t));
    return textureSample(t, s, vec2(p.x/d.x, 1.-p.y/d.y));
}
"#;

// ── Graph ─────────────────────────────────────────────────────────────────────

struct MixerGraph {
    graph:    Graph,
    node_a:   NodeId,  // Syphon A passthrough
    node_b:   NodeId,  // Syphon B passthrough
    node_mix: NodeId,  // Crossfade
    node_out: NodeId,  // PixelsOut
}

impl MixerGraph {
    fn new() -> Self {
        let mut g = Graph::new();
        let node_a   = g.add_node(NodeKind::ShaderSource);
        let node_b   = g.add_node(NodeKind::ShaderSource);
        let node_mix = g.add_node(NodeKind::Crossfade);
        let node_out = g.add_node(NodeKind::PixelsOut);

        g.connect_named(node_a,   "out", node_mix, "a").unwrap();
        g.connect_named(node_b,   "out", node_mix, "b").unwrap();
        g.connect_named(node_mix, "out", node_out, "in").unwrap();

        Self { graph: g, node_a, node_b, node_mix, node_out }
    }
}

// ── Instrument ────────────────────────────────────────────────────────────────

struct Mixer {
    args:    Args,
    runtime: Option<WgpuRuntime>,
    preview: Option<PreviewSink>,
    window:  Option<Arc<Window>>,
    watcher: Option<AssetWatcher>,
    mg:      MixerGraph,
    param_store: Arc<Mutex<ParamStore>>,
    midi:       Option<midir::MidiInputConnection<()>>,
    frame:   u64,
    start:   Instant,
    last:    Instant,

    shader_a:      Option<String>,
    shader_b:      Option<String>,
    crossfade:     Option<String>,

    #[cfg(target_os = "macos")]
    syphon_a: Option<SyphonReceiver>,
    #[cfg(target_os = "macos")]
    syphon_b: Option<SyphonReceiver>,
    #[cfg(target_os = "macos")]
    syphon_out: Option<SyphonSink>,
    #[cfg(target_os = "macos")]
    mtl_device_ptr: *mut std::ffi::c_void,
    #[cfg(target_os = "macos")]
    syphon_initialized: bool,
}

#[cfg(target_os = "macos")]
unsafe impl Send for Mixer {}

impl Mixer {
    fn new() -> Self {
        Self {
            args:    parse_args(),
            runtime: None,
            preview: None,
            window:  None,
            watcher: None,
            mg:      MixerGraph::new(),
            param_store: Arc::new(Mutex::new(build_param_store())),
            midi:    None,
            frame:   0,
            start:   Instant::now(),
            last:    Instant::now(),
            shader_a:  None,
            shader_b:  None,
            crossfade: None,
            #[cfg(target_os = "macos")]
            syphon_a: None,
            #[cfg(target_os = "macos")]
            syphon_b: None,
            #[cfg(target_os = "macos")]
            syphon_out: None,
            #[cfg(target_os = "macos")]
            mtl_device_ptr: std::ptr::null_mut(),
            #[cfg(target_os = "macos")]
            syphon_initialized: false,
        }
    }

    fn tick(&mut self) {
        if Instant::now().duration_since(self.last) < frame_budget() { return; }
        self.last = Instant::now();

        // Hot-reload
        if let Some(ref mut w) = self.watcher {
            if !w.drain().is_empty() {
                self.shader_a  = std::fs::read_to_string("assets/shaders/source_a.frag").ok();
                self.shader_b  = std::fs::read_to_string("assets/shaders/source_b.frag").ok();
                self.crossfade = std::fs::read_to_string("assets/shaders/crossfade.frag").ok();
                log::info!("Hot-reloaded shaders");
            }
        }

        // Syphon deferred init (frame 5)
        #[cfg(target_os = "macos")]
        if !self.syphon_initialized && !self.mtl_device_ptr.is_null() && self.frame >= 5 {
            self.syphon_initialized = true;
            let servers = SyphonReceiver::list_servers(self.mtl_device_ptr);
            if servers.is_empty() {
                log::info!("Syphon: no sources found");
            } else {
                log::info!("Syphon sources ({}):", servers.len());
                for s in &servers {
                    log::info!("  '{}' from '{}'", s.name, s.app);
                }
            }
            if let (Some(ref name), Some(ref r)) = (&self.args.syphon_a.clone(), &self.runtime) {
                self.syphon_a = SyphonReceiver::connect(name, self.mtl_device_ptr, &r.ctx.device, &r.ctx.queue)
                    .map(|r| { log::info!("Syphon A: '{name}' connected"); r })
                    .map_err(|e| log::warn!("Syphon A: {e}")).ok();
            }
            if let (Some(ref name), Some(ref r)) = (&self.args.syphon_b.clone(), &self.runtime) {
                self.syphon_b = SyphonReceiver::connect(name, self.mtl_device_ptr, &r.ctx.device, &r.ctx.queue)
                    .map(|r| { log::info!("Syphon B: '{name}' connected"); r })
                    .map_err(|e| log::warn!("Syphon B: {e}")).ok();
            }
        }

        // Poll Syphon inputs
        #[cfg(target_os = "macos")]
        if let (Some(ref mut sa), Some(ref r)) = (&mut self.syphon_a, &self.runtime) {
            sa.poll_with_device(&r.ctx.device, &r.ctx.queue);
        }
        #[cfg(target_os = "macos")]
        if let (Some(ref mut sb), Some(ref r)) = (&mut self.syphon_b, &self.runtime) {
            sb.poll_with_device(&r.ctx.device, &r.ctx.queue);
        }

        let (Some(ref mut runtime), Some(ref mut preview)) =
            (&mut self.runtime, &mut self.preview) else { return };

        let tbar = {
            let mut store = self.param_store.lock().unwrap();
            store.step_frame();  // advance smoother: targets → values
            store.get("u_tbar").unwrap_or(0.0)
        };
        let time  = self.start.elapsed().as_secs_f32();
        let ctx   = FrameCtx {
            width: self.args.width, height: self.args.height,
            time, frame: self.frame, sample_count: self.args.msaa,
        };

        let mut configs: HashMap<NodeId, NodeConfig> = HashMap::new();

        // Node A config
        let mut cfg_a = NodeConfig::default();
        cfg_a.frag_shader = self.shader_a.clone();
        #[cfg(target_os = "macos")]
        if let Some(ref sa) = self.syphon_a {
            cfg_a.input_textures[0] = sa.texture_arc();
        }
        configs.insert(self.mg.node_a, cfg_a);

        // Node B config
        let mut cfg_b = NodeConfig::default();
        cfg_b.frag_shader = self.shader_b.clone();
        #[cfg(target_os = "macos")]
        if let Some(ref sb) = self.syphon_b {
            cfg_b.input_textures[0] = sb.texture_arc();
        }
        configs.insert(self.mg.node_b, cfg_b);

        // Mix node config
        let mut cfg_mix = NodeConfig::default();
        cfg_mix.frag_shader = self.crossfade.clone();
        cfg_mix.uniforms.insert("u_tbar".to_owned(),    tbar);
        cfg_mix.uniforms.insert("u_softness".to_owned(), 0.0);
        configs.insert(self.mg.node_mix, cfg_mix);

        configs.insert(self.mg.node_out, NodeConfig::default());

        let plan = match self.mg.graph.compile() {
            Ok(p)  => p,
            Err(e) => { log::error!("Graph: {e}"); return; }
        };

        // Always render to preview window
        if let Err(e) = runtime.execute_frame(&self.mg.graph, &plan, &configs, &ctx, preview) {
            log::error!("execute_frame: {e}");
        }
        // Also push to Syphon output if available
        #[cfg(target_os = "macos")]
        if let Some(ref mut so) = self.syphon_out {
            if let Err(e) = runtime.execute_frame(&self.mg.graph, &plan, &configs, &ctx, so) {
                log::error!("execute_frame syphon: {e}");
            }
        }

        if self.frame % TARGET_FPS as u64 == 0 {
            log::info!("t={:.1}s | t-bar={:.2} | MIDI CC1→u_tbar", time, tbar);
        }

        self.frame += 1;
    }
}

impl ApplicationHandler for Mixer {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let win = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("scheng-mixer")
                    .with_inner_size(winit::dpi::LogicalSize::new(self.args.width, self.args.height))
            ).unwrap()
        );

        let runtime = WgpuRuntime::new(self.args.width, self.args.height)
            .expect("wgpu runtime");

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
        // Connect MIDI using midir directly (same pattern as shadecore)
        let midi_store = Arc::clone(&self.param_store);
        self.midi = (|| -> Option<midir::MidiInputConnection<()>> {
            let mut midi_in = MidirInput::new("scheng-mixer").ok()?;
            midi_in.ignore(Ignore::None);
            let ports = midi_in.ports();
            if ports.is_empty() {
                log::warn!("MIDI: no ports found");
                return None;
            }
            for p in &ports {
                if let Ok(name) = midi_in.port_name(p) {
                    log::info!("MIDI port: '{name}'");
                }
            }
            let port = ports.into_iter().next()?;
            let name = midi_in.port_name(&port).unwrap_or_default();
            let conn = midi_in.connect(&port, "scheng-mixer-in", move |_ts, msg, _| {
                if msg.len() == 3 && (msg[0] & 0xF0) == 0xB0 {
                    let cc  = msg[1];
                    let val = msg[2] as f32 / 127.0;
                    log::info!("[MIDI] CC{cc} = {val:.2}");
                    if let Ok(mut s) = midi_store.lock() {
                        let _ = s.set_by_midi_cc(cc, msg[2]);
                    }
                }
            }, ()).ok()?;
            log::info!("MIDI connected: '{name}'");
            Some(conn)
        })();

        // Syphon output
        #[cfg(target_os = "macos")]
        {
            self.syphon_out = scheng_output_syphon::SyphonSink::new("scheng-mixer").map(|s| { log::info!("Syphon out: 'scheng-mixer' ready"); s })
             .map_err(|e| log::warn!("Syphon out: {e}")).ok();

            // Extract MTL device pointer
            unsafe {
                use foreign_types_shared::ForeignType;
                runtime.ctx.device.as_hal::<wgpu::hal::api::Metal, _, ()>(|hal| {
                    if let Some(d) = hal {
                        let guard = d.raw_device().lock();
                        self.mtl_device_ptr = guard.as_ptr() as *mut std::ffi::c_void;
                    }
                });
                scheng_input_syphon::ffi::scheng_syphon_directory_init();
            }
        }

        // Load shaders
        self.shader_a  = std::fs::read_to_string("assets/shaders/source_a.frag").ok();
        self.shader_b  = std::fs::read_to_string("assets/shaders/source_b.frag").ok();
        self.crossfade = std::fs::read_to_string("assets/shaders/crossfade.frag").ok();

        // Hot-reload watcher
        self.watcher = AssetWatcher::new("assets")
            .map_err(|e| log::warn!("Watcher: {e}")).ok();

        log::info!("{}×{} @ {}fps | T-bar: MIDI CC1 | A={:?} B={:?}",
            self.args.width, self.args.height, TARGET_FPS,
            self.args.syphon_a, self.args.syphon_b);

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
                if let (Some(ref mut rt), Some(ref mut pv)) = (&mut self.runtime, &mut self.preview) {
                    pv.config.width  = sz.width;
                    pv.config.height = sz.height;
                    pv.surface.configure(&rt.ctx.device, &pv.config);
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
    EventLoop::new().unwrap().run_app(&mut Mixer::new()).unwrap();
}
