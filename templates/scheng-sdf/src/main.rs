use std::{
    collections::HashMap, path::PathBuf,
    sync::{Arc, Mutex}, time::Instant,
};
use scheng_graph::{Graph, NodeId, NodeKind};
use scheng_hotreload::watcher::AssetWatcher;
use scheng_param_store::{ParamStore, ParamSchema, schema::ParamDef};
use scheng_runtime_wgpu::{executor::{NodeConfig, OutputSink}, FrameCtx, WgpuRuntime};
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

const FRAME_CAP_NS:   u64  = 1_000_000_000 / 120;
const DEFAULT_WIDTH:  u32  = 1280;
const DEFAULT_HEIGHT: u32  = 720;

// Resolve shader directory relative to the executable, not cwd.
// This ensures shaders are found whether run via `cargo run` or directly.
fn shader_dir() -> PathBuf {
    // First try: next to the executable (installed / release)
    if let Ok(exe) = std::env::current_exe() {
        let candidate = exe.parent().unwrap_or(std::path::Path::new("."))
            .join("assets").join("shaders");
        if candidate.is_dir() { return candidate; }
    }
    // Second try: cwd (cargo run from project root)
    let candidate = PathBuf::from("assets/shaders");
    if candidate.is_dir() { return candidate; }
    // Fallback — will produce a clear error in load_shaders
    candidate
}

#[derive(Debug)]
struct Args { width: u32, height: u32, msaa: u32, render_scale: f32 }
impl Default for Args {
    fn default() -> Self { Self { width: DEFAULT_WIDTH, height: DEFAULT_HEIGHT, msaa: 1, render_scale: 1.0 } }
}
fn parse_args() -> Args {
    let raw: Vec<String> = std::env::args().collect();
    let mut a = Args::default();
    let mut i = 1;
    while i < raw.len() {
        match raw[i].as_str() {
            "--width"        => { i+=1; a.width        = raw[i].parse().unwrap_or(DEFAULT_WIDTH); }
            "--height"       => { i+=1; a.height       = raw[i].parse().unwrap_or(DEFAULT_HEIGHT); }
            "--msaa"         => { i+=1; a.msaa         = raw[i].parse().unwrap_or(1); }
            "--render-scale" => { i+=1; a.render_scale = raw[i].parse().unwrap_or(1.0); }
            other => log::warn!("Unknown arg: {other}"),
        }
        i += 1;
    }
    a
}

fn render_size(w: u32, h: u32, scale: f32) -> (u32, u32) {
    let rw = ((w as f32 * scale) as u32).max(64);
    let rh = ((h as f32 * scale) as u32).max(36);
    ((rw+1)&!1, (rh+1)&!1)
}

fn make_def(name: &str, default: f32, cc: u8) -> ParamDef {
    ParamDef { name: name.into(), ty: "float".into(),
        min: 0.0, max: 1.0, default, smooth: 0.05,
        midi_cc: Some(cc), midi_channel: None,
        osc_addr: Some(format!("/scheng/{name}")),
        node_label: None, description: None }
}

fn build_param_store() -> ParamStore {
    ParamStore::new(ParamSchema { version: 1, params: vec![
        make_def("u_p1", 0.35, 1),
        make_def("u_p2", 0.40, 2),
        make_def("u_p3", 0.40, 3),
        make_def("u_p4", 0.00, 4),
        make_def("u_p5", 0.40, 5),
        make_def("u_p6", 0.40, 6),
        make_def("u_p7", 0.00, 7),  // 0 = key mode (black bg), 1 = abstract
        make_def("u_p8", 0.50, 8),
    ]})
}

// ── PreviewSink ───────────────────────────────────────────────────────────
struct PreviewSink {
    surface: wgpu::Surface<'static>,
    config:  wgpu::SurfaceConfiguration,
    pipeline: Option<wgpu::RenderPipeline>,
    sampler:  Option<wgpu::Sampler>,
}
impl PreviewSink {
    fn new(s: wgpu::Surface<'static>, c: wgpu::SurfaceConfiguration) -> Self {
        Self { surface: s, config: c, pipeline: None, sampler: None }
    }
    fn ensure(&mut self, device: &wgpu::Device) {
        if self.pipeline.is_some() { return; }
        let sh = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit"), source: wgpu::ShaderSource::Wgsl(BLIT.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None, entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None, bind_group_layouts: &[&bgl], push_constant_ranges: &[],
        });
        self.pipeline = Some(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None, layout: Some(&layout),
            vertex: wgpu::VertexState { module: &sh, entry_point: Some("vs_main"),
                compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState { module: &sh, entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.config.format,
                    blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: Default::default(), depth_stencil: None,
            multisample: Default::default(), multiview: None, cache: None,
        }));
        self.sampler = Some(device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        }));
    }
}
impl OutputSink for PreviewSink {
    fn present(&mut self, _id: NodeId, target: &scheng_runtime_wgpu::RenderTarget,
        _ctx: &FrameCtx, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.ensure(device);
        let frame = match self.surface.get_current_texture() { Ok(f)=>f, Err(_)=>return };
        let view  = frame.texture.create_view(&Default::default());
        let pl    = self.pipeline.as_ref().unwrap();
        let bg    = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None, layout: &pl.get_bind_group_layout(0), entries: &[
                wgpu::BindGroupEntry { binding: 0,
                    resource: wgpu::BindingResource::TextureView(&target.sample_view) },
                wgpu::BindGroupEntry { binding: 1,
                    resource: wgpu::BindingResource::Sampler(self.sampler.as_ref().unwrap()) },
            ],
        });
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view, resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(pl);
            pass.set_bind_group(0, &bg, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit(std::iter::once(enc.finish()));
        frame.present();
    }
}

const BLIT: &str = r#"
struct V { @builtin(position) p: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> V {
    var p = array<vec2<f32>,3>(vec2(-1.,-3.),vec2(3.,1.),vec2(-1.,1.));
    var u = array<vec2<f32>,3>(vec2(0.,2.),vec2(2.,0.),vec2(0.,0.));
    return V(vec4<f32>(p[i],0.,1.), u[i]);
}
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@fragment fn fs_main(v: V) -> @location(0) vec4<f32> {
    return textureSample(t, s, vec2(v.uv.x, 1.0 - v.uv.y));
}
"#;

// ── Instrument ────────────────────────────────────────────────────────────
struct SdfInstrument {
    args:        Args,
    runtime:     Option<WgpuRuntime>,
    preview:     Option<PreviewSink>,
    window:      Option<Arc<Window>>,
    watcher:     Option<AssetWatcher>,
    param_store: Arc<Mutex<ParamStore>>,
    midi:        Option<midir::MidiInputConnection<()>>,
    graph:       Graph,
    node_src:    NodeId,
    node_out:    NodeId,
    shaders:     Vec<PathBuf>,
    current:     usize,
    shader_src:      Option<String>,
    last_good_shader: Option<String>,
    frame:       u64,
    start:       Instant,
    last:        Instant,
    fps_timer:   Instant,
    fps_count:   u32,
    #[cfg(target_os = "macos")]
    syphon_out:  Option<SyphonSink>,
}

impl SdfInstrument {
    fn new() -> Self {
        let mut g = Graph::new();
        let node_src = g.add_node(NodeKind::ShaderSource);
        let node_out = g.add_node(NodeKind::PixelsOut);
        g.connect_named(node_src, "out", node_out, "in").unwrap();
        Self {
            args: parse_args(), runtime: None, preview: None, window: None,
            watcher: None, midi: None,
            param_store: Arc::new(Mutex::new(build_param_store())),
            graph: g, node_src, node_out,
            shaders: Vec::new(), current: 0, shader_src: None, last_good_shader: None,
            frame: 0, start: Instant::now(), last: Instant::now(),
            fps_timer: Instant::now(), fps_count: 0,
            #[cfg(target_os = "macos")] syphon_out: None,
        }
    }

    fn load_shaders(&mut self) {
        let dir = shader_dir();
        log::info!("Loading shaders from: {}", dir.display());

        match std::fs::read_dir(&dir) {
            Err(e) => {
                log::error!("Cannot read shader dir {}: {e}", dir.display());
                log::error!("Run from the scheng-sdf project root: cd /Users/tgm/Documents/SPLASH/scheng-sdf && cargo run --release");
                return;
            }
            Ok(entries) => {
                let mut paths: Vec<PathBuf> = entries
                    .flatten()
                    .filter(|e| e.path().extension().map_or(false, |x| x=="frag"))
                    .map(|e| e.path())
                    .collect();
                paths.sort();
                self.shaders = paths;
            }
        }

        log::info!("Found {} shaders:", self.shaders.len());
        for (i, p) in self.shaders.iter().enumerate() {
            log::info!("  [{}] {}", i+1, p.file_name().unwrap_or_default().to_string_lossy());
        }

        if self.shaders.is_empty() {
            log::error!("No .frag files found — shaders must be in {}", dir.display());
            return;
        }

        self.current = self.current.min(self.shaders.len() - 1);
        self.reload_current();
    }

    fn reload_current(&mut self) {
        if self.shaders.is_empty() {
            log::warn!("No shaders loaded");
            return;
        }
        let path = &self.shaders[self.current];
        match std::fs::read_to_string(path) {
            Ok(src) => {
                self.shader_src = Some(src);
                log::info!("▶ [{}/{}] {}",
                    self.current + 1,
                    self.shaders.len(),
                    path.file_name().unwrap_or_default().to_string_lossy());
            }
            Err(e) => {
                log::error!("Failed to read {}: {e}", path.display());
                self.shader_src = None;
            }
        }
    }

    fn next_shader(&mut self) {
        if self.shaders.is_empty() { log::warn!("No shaders to cycle"); return; }
        self.current = (self.current + 1) % self.shaders.len();
        self.reload_current();
    }

    fn prev_shader(&mut self) {
        if self.shaders.is_empty() { log::warn!("No shaders to cycle"); return; }
        self.current = self.current.checked_sub(1).unwrap_or(self.shaders.len() - 1);
        self.reload_current();
    }

    fn print_list(&self) {
        log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        log::info!("  SCENES ({} loaded)", self.shaders.len());
        for (i, p) in self.shaders.iter().enumerate() {
            let mark = if i == self.current { "▶" } else { " " };
            log::info!("  {} [{}] {}", mark, i+1, p.file_name().unwrap_or_default().to_string_lossy());
        }
        log::info!("  KEYS: →/] next   ←/[ prev   R reload   F list");
        log::info!("  CC7 = 0 → black bg (Resolume key mode)");
        log::info!("  CC7 > 0 → coloured bg (abstract mode)");
        log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    }

    fn tick(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last).as_nanos() < FRAME_CAP_NS as u128 { return; }
        self.last = now;

        self.fps_count += 1;
        if self.fps_timer.elapsed().as_secs_f32() >= 1.0 {
            // Extract value BEFORE format — two lock() calls in same expression = deadlock
            let p7   = self.param_store.lock().unwrap().get("u_p7").unwrap_or(0.0);
            let mode = if p7 < 0.1 { "KEY" } else { "abstract" };
            log::info!("{:.0} fps | scene {}/{} | CC7={:.2} ({})",
                self.fps_count as f32 / self.fps_timer.elapsed().as_secs_f32(),
                self.current + 1, self.shaders.len(),
                p7, mode
            );
            self.fps_count = 0;
            self.fps_timer = Instant::now();
        }

        if let Some(ref mut w) = self.watcher {
            if !w.drain().is_empty() {
                self.load_shaders();
            }
        }

        let shader_src = self.shader_src.clone();
        let uniforms = {
            let mut s = self.param_store.lock().unwrap();
            s.step_frame();
            s.all_values().clone()
        };
        let time  = self.start.elapsed().as_secs_f32();
        let frame = self.frame;
        let (rw, rh) = render_size(self.args.width, self.args.height, self.args.render_scale);

        let (Some(ref mut runtime), Some(ref mut preview)) =
            (&mut self.runtime, &mut self.preview) else { return };

        let ctx = FrameCtx { width: rw, height: rh, time, frame, sample_count: self.args.msaa };

        let mut configs = HashMap::new();
        let mut cfg = NodeConfig::default();
        cfg.frag_shader = shader_src;
        cfg.uniforms    = uniforms;
        configs.insert(self.node_src, cfg);
        configs.insert(self.node_out, NodeConfig::default());

        let plan = match self.graph.compile() {
            Ok(p)  => p,
            Err(e) => { log::error!("Graph: {e}"); return; }
        };

        let exec_result = runtime.execute_frame(&self.graph, &plan, &configs, &ctx, preview);
        match exec_result {
            Ok(_) => {
                // Shader ran successfully — record as last known good
                if let Some(ref src) = self.shader_src {
                    self.last_good_shader = Some(src.clone());
                }
            }
            Err(ref e) => {
                log::error!("Shader error: {e}");
                log::warn!("Falling back to last good shader");
                if self.last_good_shader.is_some() {
                    self.shader_src = self.last_good_shader.clone();
                } else {
                    log::warn!("No fallback available — using built-in gradient");
                    self.shader_src = None;
                }
            }
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

impl ApplicationHandler for SdfInstrument {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let win = Arc::new(event_loop.create_window(
            Window::default_attributes()
                .with_title("scheng-sdf  |  →/] next scene  |  ←/[ prev  |  CC7=0 key mode")
                .with_inner_size(winit::dpi::LogicalSize::new(self.args.width, self.args.height))
        ).unwrap());

        let runtime = WgpuRuntime::new(self.args.width, self.args.height).expect("wgpu init failed");
        let surface = runtime.ctx.instance.create_surface(win.clone()).expect("surface failed");
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

        // ── MIDI ─────────────────────────────────────────────────────────
        let store = Arc::clone(&self.param_store);
        let midi_result = (|| -> Result<midir::MidiInputConnection<()>, String> {
            let mut m = MidirInput::new("scheng-sdf")
                .map_err(|e| format!("MidiInput::new failed: {e}"))?;
            m.ignore(Ignore::None);
            let ports = m.ports();
            if ports.is_empty() {
                return Err("No MIDI ports found. Connect a device or enable IAC Driver in Audio MIDI Setup.".into());
            }
            log::info!("MIDI ports available:");
            for p in &ports {
                log::info!("  • {}", m.port_name(p).unwrap_or_else(|_| "?".into()));
            }
            let port = ports.into_iter().next().unwrap();
            let name = m.port_name(&port).unwrap_or_else(|_| "?".into());
            let conn = m.connect(&port, "sdf-midi", move |_ts, msg, _| {
                if msg.len() == 3 && (msg[0] & 0xF0) == 0xB0 {
                    let cc  = msg[1];
                    let val = msg[2];
                    log::info!("[MIDI] CC{cc} = {val}");
                    store.lock().unwrap().set_by_midi_cc(cc, val).ok();
                }
            }, ()).map_err(|e| format!("MIDI connect failed: {e}"))?;
            Ok(conn)
        })();
        match midi_result {
            Ok(conn)  => { log::info!("MIDI: connected"); self.midi = Some(conn); }
            Err(msg)  => log::warn!("MIDI: {msg}"),
        }

        // ── Syphon ───────────────────────────────────────────────────────
        #[cfg(target_os = "macos")] {
            self.syphon_out = SyphonSink::new("scheng-sdf")
                .map(|s| { log::info!("Syphon: 'scheng-sdf' ready"); s })
                .map_err(|e| log::warn!("Syphon: {e}")).ok();
        }

        // ── Shaders ──────────────────────────────────────────────────────
        self.watcher = AssetWatcher::new("assets").ok();
        self.load_shaders();
        self.print_list();

        let (rw, rh) = render_size(self.args.width, self.args.height, self.args.render_scale);
        log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        log::info!("  scheng-sdf  display={}×{}  render={}×{}  MSAA {}×",
                   self.args.width, self.args.height, rw, rh, self.args.msaa);
        log::info!("  KEYS  →/] next   ←/[ prev   R reload   F list");
        log::info!("  CC1=speed  CC2=density  CC3=shape  CC4=hue");
        log::info!("  CC5=param5 CC6=param6   CC7=bg     CC8=param8");
        log::info!("  CC7=0 → KEY MODE (black bg for Resolume)");
        log::info!("  CC7>0 → ABSTRACT MODE (coloured bg)");
        log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        self.preview = Some(PreviewSink::new(surface, config));
        self.runtime = Some(runtime);
        self.window  = Some(win);
        self.start   = Instant::now();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::KeyboardInput {
                event: KeyEvent {
                    physical_key,
                    state: ElementState::Pressed,
                    ..
                },
                ..
            } => {
                log::info!("[KEY] {:?}", physical_key);
                match physical_key {
                    PhysicalKey::Code(KeyCode::Escape)       => event_loop.exit(),
                    PhysicalKey::Code(KeyCode::ArrowRight)   |
                    PhysicalKey::Code(KeyCode::BracketRight) => self.next_shader(),
                    PhysicalKey::Code(KeyCode::ArrowLeft)    |
                    PhysicalKey::Code(KeyCode::BracketLeft)  => self.prev_shader(),
                    PhysicalKey::Code(KeyCode::KeyR)         => self.reload_current(),
                    PhysicalKey::Code(KeyCode::KeyF)         => self.print_list(),
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
                if let Some(ref w) = self.window { w.request_redraw(); }
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
    EventLoop::new().unwrap().run_app(&mut SdfInstrument::new()).unwrap();
}
