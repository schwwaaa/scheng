//! scheng-playground
//!
//! Interactive multi-shader explorer.
//! Drop any `.frag` file into `assets/shaders/`, cycle through them
//! with arrow keys, and all 8 MIDI CC params are pre-wired.
//!
//! # Run
//! ```bash
//! cargo run --release
//! cargo run --release -- --width 1920 --height 1080
//! ```
//!
//! # Keyboard
//!
//! | Key      | Action                        |
//! |----------|-------------------------------|
//! | `→` / `]`| Next shader                   |
//! | `←` / `[`| Previous shader               |
//! | `R`      | Reload all shaders from disk  |
//! | `F`      | Print shader list to terminal |
//! | `Escape` | Quit                          |
//!
//! # MIDI
//!
//! CC1–CC8 map to `u_p1`–`u_p8` in every shader (0.0–1.0, smoothed).
//! Declare any you need at the top of your shader:
//!
//! ```glsl
//! uniform float u_p1;  // CC1
//! uniform float u_p2;  // CC2
//! // ...
//! ```
//!
//! # Writing shaders
//!
//! Start from `assets/shaders/08_template.frag`. All standard scheng
//! uniforms are available: `uTime`, `uFrame`, `uResolution`, `v_uv`, `fragColor`.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

use scheng_graph::{Graph, NodeId, NodeKind};
use scheng_hotreload::watcher::AssetWatcher;
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

// ── Constants ─────────────────────────────────────────────────────────────
const FRAME_CAP_NS:   u64  = 1_000_000_000 / 120;
const DEFAULT_WIDTH:  u32  = 1280;
const DEFAULT_HEIGHT: u32  = 720;
const SHADER_DIR:     &str = "assets/shaders";

// ── Args ──────────────────────────────────────────────────────────────────
#[derive(Debug)]
struct Args {
    width:  u32,
    height: u32,
    msaa:   u32,
}
impl Default for Args {
    fn default() -> Self { Self { width: DEFAULT_WIDTH, height: DEFAULT_HEIGHT, msaa: 1 } }
}
fn parse_args() -> Args {
    let raw: Vec<String> = std::env::args().collect();
    let mut a = Args::default();
    let mut i = 1;
    while i < raw.len() {
        match raw[i].as_str() {
            "--width"  => { i += 1; a.width  = raw[i].parse().unwrap_or(DEFAULT_WIDTH); }
            "--height" => { i += 1; a.height = raw[i].parse().unwrap_or(DEFAULT_HEIGHT); }
            "--msaa"   => { i += 1; a.msaa   = raw[i].parse().unwrap_or(1); }
            other      => log::warn!("Unknown arg: {other}"),
        }
        i += 1;
    }
    a
}

// ── ParamStore ────────────────────────────────────────────────────────────
fn build_param_store() -> ParamStore {
    let params = (1u8..=8).map(|n| ParamDef {
        name:         format!("u_p{n}"),
        ty:           "float".into(),
        min:          0.0,
        max:          1.0,
        default:      0.5,
        smooth:       0.05,
        midi_cc:      Some(n),
        midi_channel: None,
        osc_addr:     Some(format!("/scheng/p{n}")),
        node_label:   None,
        description:  None,
    }).collect();
    ParamStore::new(ParamSchema { version: 1, params })
}

// ── Shader list ───────────────────────────────────────────────────────────

/// Load all .frag files from SHADER_DIR, sorted by filename.
fn load_shader_list() -> Vec<(PathBuf, Option<String>)> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(SHADER_DIR)
        .unwrap_or_else(|_| {
            log::warn!("shader dir '{}' not found", SHADER_DIR);
            std::fs::read_dir(".").unwrap()
        })
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "frag").unwrap_or(false))
        .collect();

    entries.sort();

    entries.into_iter().map(|path| {
        let src = std::fs::read_to_string(&path).ok();
        (path, src)
    }).collect()
}

fn shader_name(path: &PathBuf) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}

// ── Preview sink ──────────────────────────────────────────────────────────
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
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        }));
    }
}
impl OutputSink for PreviewSink {
    fn present(&mut self, _id: NodeId, target: &scheng_runtime_wgpu::RenderTarget,
        _ctx: &FrameCtx, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.configure(device);
        let frame = match self.surface.get_current_texture() { Ok(f) => f, Err(_) => return };
        let view  = frame.texture.create_view(&Default::default());
        let pl    = self.pipeline.as_ref().unwrap();
        let bg    = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit_bg"), layout: &pl.get_bind_group_layout(0),
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
            pass.set_pipeline(pl);
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

// ── Instrument ────────────────────────────────────────────────────────────
struct Playground {
    args:         Args,
    runtime:      Option<WgpuRuntime>,
    preview:      Option<PreviewSink>,
    window:       Option<Arc<Window>>,
    watcher:      Option<AssetWatcher>,
    param_store:  Arc<Mutex<ParamStore>>,
    midi:         Option<midir::MidiInputConnection<()>>,

    // Graph — single ShaderSource → PixelsOut
    graph:        Graph,
    node_shader:  NodeId,
    node_out:     NodeId,

    // Shader list
    shaders:      Vec<(PathBuf, Option<String>)>,
    current:      usize,

    // Timing
    frame:        u64,
    start:        Instant,
    last:         Instant,
    fps_timer:    Instant,
    fps_count:    u32,
    fps:          f32,

    #[cfg(target_os = "macos")]
    syphon_out: Option<SyphonSink>,
}

impl Playground {
    fn new() -> Self {
        let mut g = Graph::new();
        let node_shader = g.add_node(NodeKind::ShaderSource);
        let node_out    = g.add_node(NodeKind::PixelsOut);
        g.connect_named(node_shader, "out", node_out, "in").unwrap();

        Self {
            args:        parse_args(),
            runtime:     None,
            preview:     None,
            window:      None,
            watcher:     None,
            param_store: Arc::new(Mutex::new(build_param_store())),
            midi:        None,
            graph:       g,
            node_shader,
            node_out,
            shaders:     Vec::new(),
            current:     0,
            frame:       0,
            start:       Instant::now(),
            last:        Instant::now(),
            fps_timer:   Instant::now(),
            fps_count:   0,
            fps:         0.0,
            #[cfg(target_os = "macos")]
            syphon_out:  None,
        }
    }

    fn current_name(&self) -> String {
        self.shaders.get(self.current)
            .map(|(p, _)| shader_name(p))
            .unwrap_or_else(|| "builtin".into())
    }

    fn current_src(&self) -> Option<String> {
        self.shaders.get(self.current).and_then(|(_, s)| s.clone())
    }

    fn go_next(&mut self) {
        if self.shaders.is_empty() { return; }
        self.current = (self.current + 1) % self.shaders.len();
        self.log_current();
        self.update_window_title();
    }

    fn go_prev(&mut self) {
        if self.shaders.is_empty() { return; }
        self.current = if self.current == 0 { self.shaders.len() - 1 } else { self.current - 1 };
        self.log_current();
        self.update_window_title();
    }

    fn reload_shaders(&mut self) {
        let name_before = self.current_name();
        self.shaders = load_shader_list();
        // Stay on same shader by name if possible
        if let Some(idx) = self.shaders.iter().position(|(p, _)| shader_name(p) == name_before) {
            self.current = idx;
        } else {
            self.current = self.current.min(self.shaders.len().saturating_sub(1));
        }
        log::info!("Reloaded {} shaders from '{}'", self.shaders.len(), SHADER_DIR);
        self.log_current();
        self.update_window_title();
    }

    fn list_shaders(&self) {
        log::info!("── Shaders in {} ──────────────────────────", SHADER_DIR);
        for (i, (path, src)) in self.shaders.iter().enumerate() {
            let mark   = if i == self.current { "▶" } else { " " };
            let status = if src.is_some() { "✓" } else { "✗ missing" };
            log::info!("  {} [{}] {} {}", mark, i + 1, shader_name(path), status);
        }
        log::info!("────────────────────────────────────────────");
    }

    fn log_current(&self) {
        let name = self.current_name();
        log::info!(
            "▶ [{}/{}] {} | CC1–CC8 → u_p1–u_p8",
            self.current + 1, self.shaders.len(), name
        );
    }

    fn update_window_title(&self) {
        if let Some(ref win) = self.window {
            let title = format!(
                "scheng-playground — [{}/{}] {}",
                self.current + 1, self.shaders.len(), self.current_name()
            );
            win.set_title(&title);
        }
    }

    fn tick(&mut self) {
        let now     = Instant::now();
        let elapsed = now.duration_since(self.last);
        if elapsed.as_nanos() < FRAME_CAP_NS as u128 { return; }
        self.last = now;

        // FPS counter
        self.fps_count += 1;
        let fps_elapsed = self.fps_timer.elapsed().as_secs_f32();
        if fps_elapsed >= 1.0 {
            self.fps       = self.fps_count as f32 / fps_elapsed;
            self.fps_count = 0;
            self.fps_timer = Instant::now();
            log::info!(
                "[{}/{}] {} | {:.0} fps | {:.1} ms",
                self.current + 1, self.shaders.len(),
                self.current_name(),
                self.fps,
                elapsed.as_secs_f32() * 1000.0
            );
        }

        // Hot-reload from watcher
        if let Some(ref mut w) = self.watcher {
            if !w.drain().is_empty() {
                self.reload_shaders();
            }
        }

        // Extract values BEFORE mutable borrows of runtime/preview
        let shader_src = self.current_src();
        let uniforms = {
            let mut store = self.param_store.lock().unwrap();
            store.step_frame();
            store.all_values().clone()
        };
        let time  = self.start.elapsed().as_secs_f32();
        let frame = self.frame;
        let (w, h, msaa) = (self.args.width, self.args.height, self.args.msaa);

        let (Some(ref mut runtime), Some(ref mut preview)) =
            (&mut self.runtime, &mut self.preview) else { return };

        let ctx = FrameCtx { width: w, height: h, time, frame, sample_count: msaa };

        let mut configs: HashMap<NodeId, NodeConfig> = HashMap::new();
        let mut cfg = NodeConfig::default();
        cfg.frag_shader = shader_src;
        cfg.uniforms    = uniforms;
        configs.insert(self.node_shader, cfg);
        configs.insert(self.node_out, NodeConfig::default());

        let plan = match self.graph.compile() {
            Ok(p) => p, Err(e) => { log::error!("Graph: {e}"); return; }
        };

        if let Err(e) = runtime.execute_frame(&self.graph, &plan, &configs, &ctx, preview) {
            log::error!("execute_frame: {e}");
        }

        #[cfg(target_os = "macos")]
        if let Some(ref mut so) = self.syphon_out {
            if let Err(e) = runtime.execute_frame(&self.graph, &plan, &configs, &ctx, so) {
                log::error!("syphon: {e}");
            }
        }

        self.frame += 1;
    }
}

impl ApplicationHandler for Playground {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let win = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("scheng-playground")
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

        // Load shaders
        self.shaders = load_shader_list();
        if self.shaders.is_empty() {
            log::warn!("No .frag files found in '{}' — using builtin gradient", SHADER_DIR);
        }

        // MIDI
        let midi_store = Arc::clone(&self.param_store);
        self.midi = (|| -> Option<midir::MidiInputConnection<()>> {
            let mut midi_in = MidirInput::new("scheng-playground").ok()?;
            midi_in.ignore(Ignore::None);
            let ports = midi_in.ports();
            if ports.is_empty() { log::warn!("MIDI: no ports found"); return None; }
            for p in &ports { if let Ok(n) = midi_in.port_name(p) { log::info!("MIDI port: '{n}'"); } }
            let port = ports.into_iter().next()?;
            let name = midi_in.port_name(&port).unwrap_or_default();
            let conn = midi_in.connect(&port, "playground-in", move |_ts, msg, _| {
                if msg.len() == 3 && (msg[0] & 0xF0) == 0xB0 {
                    midi_store.lock().unwrap().set_by_midi_cc(msg[1], msg[2]).ok();
                }
            }, ()).ok()?;
            log::info!("MIDI: connected '{name}'");
            Some(conn)
        })();

        // Syphon
        #[cfg(target_os = "macos")]
        {
            self.syphon_out = SyphonSink::new("scheng-playground")
                .map(|s| { log::info!("Syphon out: 'scheng-playground' ready"); s })
                .map_err(|e| log::warn!("Syphon out: {e}")).ok();
        }

        // File watcher
        self.watcher = AssetWatcher::new(SHADER_DIR).map_err(|e| log::warn!("Watcher: {e}")).ok();

        // Print startup info
        log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        log::info!("  scheng-playground  {}×{}  MSAA {}×", self.args.width, self.args.height, self.args.msaa);
        log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        log::info!("  →/]  next shader       ←/[  prev shader");
        log::info!("  R    reload all         F    list shaders");
        log::info!("  CC1–CC8 → u_p1–u_p8 in every shader");
        log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        self.list_shaders();

        self.preview = Some(PreviewSink::new(surface, config));
        self.runtime = Some(runtime);
        self.window  = Some(win);
        self.start   = Instant::now();

        self.update_window_title();
        self.log_current();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::KeyboardInput { event: KeyEvent {
                physical_key, state: ElementState::Pressed, ..
            }, .. } => {
                match physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => event_loop.exit(),

                    // Next shader
                    PhysicalKey::Code(KeyCode::ArrowRight) |
                    PhysicalKey::Code(KeyCode::BracketRight) => self.go_next(),

                    // Previous shader
                    PhysicalKey::Code(KeyCode::ArrowLeft) |
                    PhysicalKey::Code(KeyCode::BracketLeft) => self.go_prev(),

                    // Reload all shaders from disk
                    PhysicalKey::Code(KeyCode::KeyR) => self.reload_shaders(),

                    // List shaders
                    PhysicalKey::Code(KeyCode::KeyF) => self.list_shaders(),

                    _ => {}
                }
            }

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
    EventLoop::new().unwrap().run_app(&mut Playground::new()).unwrap();
}
