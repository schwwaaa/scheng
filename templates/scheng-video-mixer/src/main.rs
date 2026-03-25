//! scheng-video-mixer
//!
//! Two video files → MIDI T-bar crossfade → preview + Syphon output.
//!
//! # Run
//!
//! ```bash
//! cargo run --release -- --video-a clip_a.mp4 --video-b clip_b.mp4
//! ```
//!
//! Both videos loop continuously. MIDI CC1 controls the T-bar.
//!
//! # MIDI
//!
//! CC1 = T-bar (0 = full A, 127 = full B)

use std::{collections::HashMap, sync::{Arc, Mutex}, time::Instant};

use scheng_graph::{Graph, NodeId, NodeKind};
use scheng_hotreload::watcher::AssetWatcher;
use scheng_input_video::VideoDecoder;
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

#[derive(Debug, Default)]
struct Args {
    width:   u32,
    height:  u32,
    msaa:    u32,
    video_a: Option<String>,
    video_b: Option<String>,
}

fn parse_args() -> Args {
    let raw: Vec<String> = std::env::args().collect();
    let mut a = Args { width: DEFAULT_WIDTH, height: DEFAULT_HEIGHT, msaa: 1, ..Default::default() };
    let mut i = 1;
    while i < raw.len() {
        match raw[i].as_str() {
            "--width"   => { i += 1; a.width   = raw[i].parse().unwrap_or(DEFAULT_WIDTH); }
            "--height"  => { i += 1; a.height  = raw[i].parse().unwrap_or(DEFAULT_HEIGHT); }
            "--msaa"    => { i += 1; a.msaa    = raw[i].parse().unwrap_or(1); }
            "--video-a" => { i += 1; a.video_a = Some(raw[i].clone()); }
            "--video-b" => { i += 1; a.video_b = Some(raw[i].clone()); }
            other       => log::warn!("Unknown arg: {other}"),
        }
        i += 1;
    }
    a
}

// ── ParamStore ────────────────────────────────────────────────────────────────

fn build_param_store() -> ParamStore {
    ParamStore::new(ParamSchema {
        version: 1,
        params: vec![
            ParamDef {
                name: "u_tbar".into(), ty: "float".into(),
                min: 0.0, max: 1.0, default: 0.0, smooth: 0.05,
                midi_cc: Some(1), midi_channel: None,
                osc_addr: Some("/scheng/tbar".into()),
                node_label: None, description: None,
            },
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
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        }));
    }
}

impl OutputSink for PreviewSink {
    fn present(&mut self, _id: NodeId, target: &scheng_runtime_wgpu::RenderTarget,
        _ctx: &FrameCtx, device: &wgpu::Device, queue: &wgpu::Queue) {
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

struct MixerGraph {
    graph:    Graph,
    node_a:   NodeId,
    node_b:   NodeId,
    node_mix: NodeId,
    node_out: NodeId,
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

struct VideoMixer {
    args:        Args,
    runtime:     Option<WgpuRuntime>,
    preview:     Option<PreviewSink>,
    window:      Option<Arc<Window>>,
    watcher:     Option<AssetWatcher>,
    mg:          MixerGraph,
    param_store: Arc<Mutex<ParamStore>>,
    midi:        Option<midir::MidiInputConnection<()>>,
    video_a:     Option<VideoDecoder>,
    video_b:     Option<VideoDecoder>,
    crossfade:   Option<String>,
    frame:       u64,
    start:       Instant,
    last:        Instant,

    #[cfg(target_os = "macos")]
    syphon_out: Option<SyphonSink>,
}

impl VideoMixer {
    fn new() -> Self {
        Self {
            args:        parse_args(),
            runtime:     None,
            preview:     None,
            window:      None,
            watcher:     None,
            mg:          MixerGraph::new(),
            param_store: Arc::new(Mutex::new(build_param_store())),
            midi:        None,
            video_a:     None,
            video_b:     None,
            crossfade:   None,
            frame:       0,
            start:       Instant::now(),
            last:        Instant::now(),
            #[cfg(target_os = "macos")]
            syphon_out:  None,
        }
    }

    fn tick(&mut self) {
        if Instant::now().duration_since(self.last) < frame_budget() { return; }
        self.last = Instant::now();

        // Hot-reload
        if let Some(ref mut w) = self.watcher {
            if !w.drain().is_empty() {
                self.crossfade = std::fs::read_to_string("assets/shaders/crossfade.frag").ok();
                log::info!("Hot-reloaded crossfade.frag");
            }
        }

        let time = self.start.elapsed().as_secs_f32();

        // Upload video frames
        if let (Some(ref mut va), Some(ref r)) = (&mut self.video_a, &self.runtime) {
            va.upload_frame(time, &r.ctx.queue);
        }
        if let (Some(ref mut vb), Some(ref r)) = (&mut self.video_b, &self.runtime) {
            vb.upload_frame(time, &r.ctx.queue);
        }

        let (Some(ref mut runtime), Some(ref mut preview)) =
            (&mut self.runtime, &mut self.preview) else { return };

        // Step param smoother
        let tbar = {
            let mut store = self.param_store.lock().unwrap();
            store.step_frame();
            store.get("u_tbar").unwrap_or(0.0)
        };

        let ctx = FrameCtx {
            width: self.args.width, height: self.args.height,
            time, frame: self.frame, sample_count: self.args.msaa,
        };

        // Passthrough shader — Y-flip for video files
        let passthrough = "void main() { fragColor = texture(iChannel0, vec2(v_uv.x, 1.0 - v_uv.y)); }".to_string();

        let mut configs: HashMap<NodeId, NodeConfig> = HashMap::new();

        // Video A node
        let mut cfg_a = NodeConfig::default();
        cfg_a.frag_shader = Some(passthrough.clone());
        if let Some(ref va) = self.video_a {
            cfg_a.input_textures[0] = va.texture_arc();
        }
        configs.insert(self.mg.node_a, cfg_a);

        // Video B node
        let mut cfg_b = NodeConfig::default();
        cfg_b.frag_shader = Some(passthrough.clone());
        if let Some(ref vb) = self.video_b {
            cfg_b.input_textures[0] = vb.texture_arc();
        }
        configs.insert(self.mg.node_b, cfg_b);

        // Mix node
        let mut cfg_mix = NodeConfig::default();
        cfg_mix.frag_shader = self.crossfade.clone();
        cfg_mix.uniforms.insert("u_tbar".into(), tbar);
        configs.insert(self.mg.node_mix, cfg_mix);

        configs.insert(self.mg.node_out, NodeConfig::default());

        let plan = match self.mg.graph.compile() {
            Ok(p)  => p,
            Err(e) => { log::error!("Graph: {e}"); return; }
        };

        if let Err(e) = runtime.execute_frame(&self.mg.graph, &plan, &configs, &ctx, preview) {
            log::error!("execute_frame: {e}");
        }

        #[cfg(target_os = "macos")]
        if let Some(ref mut so) = self.syphon_out {
            if let Err(e) = runtime.execute_frame(&self.mg.graph, &plan, &configs, &ctx, so) {
                log::error!("syphon: {e}");
            }
        }

        if self.frame % TARGET_FPS as u64 == 0 {
            log::info!("t={:.1}s | t-bar={:.2}", time, tbar);
        }

        self.frame += 1;
    }
}

impl ApplicationHandler for VideoMixer {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let win = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("scheng-video-mixer")
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

        // Load videos
        if let Some(ref path) = self.args.video_a.clone() {
            self.video_a = VideoDecoder::open(path, &runtime.ctx.device, &runtime.ctx.queue)
                .map(|v| { log::info!("Video A: '{}' {}×{} {:.1}fps", path, v.width(), v.height(), v.fps()); v })
                .map_err(|e| log::warn!("Video A: {e}")).ok();
        } else {
            log::warn!("No --video-a specified — channel A will be black");
        }

        if let Some(ref path) = self.args.video_b.clone() {
            self.video_b = VideoDecoder::open(path, &runtime.ctx.device, &runtime.ctx.queue)
                .map(|v| { log::info!("Video B: '{}' {}×{} {:.1}fps", path, v.width(), v.height(), v.fps()); v })
                .map_err(|e| log::warn!("Video B: {e}")).ok();
        } else {
            log::warn!("No --video-b specified — channel B will be black");
        }

        // MIDI
        let midi_store = Arc::clone(&self.param_store);
        self.midi = (|| -> Option<midir::MidiInputConnection<()>> {
            let mut midi_in = MidirInput::new("scheng-video-mixer").ok()?;
            midi_in.ignore(Ignore::None);
            let ports = midi_in.ports();
            if ports.is_empty() { log::warn!("MIDI: no ports found"); return None; }
            for p in &ports { if let Ok(n) = midi_in.port_name(p) { log::info!("MIDI port: '{n}'"); } }
            let port = ports.into_iter().next()?;
            let name = midi_in.port_name(&port).unwrap_or_default();
            let conn = midi_in.connect(&port, "video-mixer-in", move |_ts, msg, _| {
                if msg.len() == 3 && (msg[0] & 0xF0) == 0xB0 {
                    log::info!("[MIDI] CC{} = {}", msg[1], msg[2]);
                    if let Ok(mut s) = midi_store.lock() {
                        let _ = s.set_by_midi_cc(msg[1], msg[2]);
                    }
                }
            }, ()).ok()?;
            log::info!("MIDI connected: '{name}'");
            Some(conn)
        })();

        // Syphon output
        #[cfg(target_os = "macos")]
        {
            self.syphon_out = SyphonSink::new("scheng-video-mixer")
                .map(|s| { log::info!("Syphon out: 'scheng-video-mixer' ready"); s })
                .map_err(|e| log::warn!("Syphon out: {e}")).ok();
        }

        self.crossfade = std::fs::read_to_string("assets/shaders/crossfade.frag").ok();
        self.watcher   = AssetWatcher::new("assets").map_err(|e| log::warn!("Watcher: {e}")).ok();

        log::info!("{}×{} @ {}fps | MIDI CC1 = T-bar", self.args.width, self.args.height, TARGET_FPS);

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
    EventLoop::new().unwrap().run_app(&mut VideoMixer::new()).unwrap();
}
