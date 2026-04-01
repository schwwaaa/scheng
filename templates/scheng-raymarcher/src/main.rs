//! scheng-raymarcher
//!
//! 3D raymarched scene rendered entirely in a fragment shader.
//! No vertex buffers. No mesh data. No depth buffer.
//! Full MIDI control of camera, lighting, and scene geometry.
//!
//! # How 3D works here
//!
//! Every pixel casts a ray from the camera into the scene.
//! The ray steps forward until it hits a surface described by
//! a Signed Distance Function (SDF). The closer the ray gets
//! to zero, the closer it is to a surface. This is raymarching.
//!
//! All geometry, lighting, shadows, and ambient occlusion run
//! in `assets/shaders/scene.frag` — edit and save to hot-reload.
//!
//! # Run
//!
//! ```bash
//! cargo run --release
//! cargo run --release -- --width 1920 --height 1080
//! cargo run --release -- --width 3840 --height 2160 --msaa 4
//! ```
//!
//! # MIDI controls
//!
//! | CC | Parameter         | Range        |
//! |----|-------------------|--------------|
//! | CC1 | Camera orbit     | 0–360°       |
//! | CC2 | Camera elevation | Low → High   |
//! | CC3 | Camera distance  | Close → Far  |
//! | CC4 | Fog density      | Clear → Dense|
//! | CC5 | Scene complexity | Simple → Complex |
//! | CC6 | Light temperature| Warm → Cool  |
//! | CC7 | Reflectivity     | Matte → Mirror |
//! | CC8 | Animation speed  | Slow → Fast  |

use std::{collections::HashMap, sync::{Arc, Mutex}, time::Instant};

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
const LOG_INTERVAL_SEC: f32 = 2.0;
const FRAME_CAP_NS:     u64  = 1_000_000_000 / 120; // 120fps ceiling
const DEFAULT_WIDTH:  u32 = 1280;
const DEFAULT_HEIGHT: u32 = 720;

// ── Args ──────────────────────────────────────────────────────────────────
#[derive(Debug)]
struct Args {
    width:  u32,
    height: u32,
    msaa:   u32,
}

impl Default for Args {
    fn default() -> Self {
        Self { width: DEFAULT_WIDTH, height: DEFAULT_HEIGHT, msaa: 1 }
    }
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
fn make_def(name: &str, min: f32, max: f32, default: f32, cc: u8, smooth: f32) -> ParamDef {
    ParamDef {
        name: name.into(), ty: "float".into(),
        min, max, default, smooth,
        midi_cc: Some(cc), midi_channel: None,
        osc_addr: Some(format!("/scheng/{name}")),
        node_label: None, description: None,
    }
}

fn build_param_store() -> ParamStore {
    ParamStore::new(ParamSchema {
        version: 1,
        params: vec![
            // Camera controls — smoother response for orbital feel
            make_def("u_cam_angle",     0.0, 1.0, 0.25, 1, 0.08),
            make_def("u_cam_elevation", 0.0, 1.0, 0.35, 2, 0.06),
            make_def("u_cam_distance",  0.0, 1.0, 0.45, 3, 0.06),
            // Scene
            make_def("u_fog",           0.0, 1.0, 0.3,  4, 0.05),
            make_def("u_complexity",    0.0, 1.0, 0.5,  5, 0.05),
            // Lighting
            make_def("u_light_temp",    0.0, 1.0, 0.4,  6, 0.05),
            make_def("u_reflectivity",  0.0, 1.0, 0.4,  7, 0.05),
            // Animation
            make_def("u_speed",         0.0, 1.0, 0.3,  8, 0.03),
        ],
    })
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

// Uses interpolated UVs — correct window resize behaviour
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
struct Raymarcher {
    args:        Args,
    runtime:     Option<WgpuRuntime>,
    preview:     Option<PreviewSink>,
    window:      Option<Arc<Window>>,
    watcher:     Option<AssetWatcher>,
    param_store: Arc<Mutex<ParamStore>>,
    midi:        Option<midir::MidiInputConnection<()>>,
    scene_src:   Option<String>,
    overlay_src: Option<String>,
    graph:       Graph,
    node_scene:   NodeId,
    node_overlay: NodeId,
    node_out:     NodeId,
    frame:       u64,
    start:       Instant,
    last:        Instant,
    fps_timer:   Instant,
    fps_count:   u32,
    fps:         f32,
    last_ms:     f32,

    #[cfg(target_os = "macos")]
    syphon_out: Option<SyphonSink>,
}

impl Raymarcher {
    fn new() -> Self {
        let mut g = Graph::new();
        let node_scene   = g.add_node(NodeKind::ShaderSource);
        let node_overlay = g.add_node(NodeKind::ShaderPass);
        let node_out     = g.add_node(NodeKind::PixelsOut);
        g.connect_named(node_scene,   "out", node_overlay, "in").unwrap();
        g.connect_named(node_overlay, "out", node_out,     "in").unwrap();

        Self {
            args:        parse_args(),
            runtime:     None,
            preview:     None,
            window:      None,
            watcher:     None,
            param_store: Arc::new(Mutex::new(build_param_store())),
            midi:        None,
            scene_src:   None,
            overlay_src: None,
            graph:       g,
            node_scene,
            node_overlay,
            node_out,
            frame:       0,
            start:       Instant::now(),
            last:        Instant::now(),
            fps_timer:   Instant::now(),
            fps_count:   0,
            fps:         0.0,
            last_ms:     0.0,
            #[cfg(target_os = "macos")]
            syphon_out:  None,
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last);
        // 120fps ceiling — prevents CPU spin while allowing full GPU throughput
        if elapsed.as_nanos() < FRAME_CAP_NS as u128 { return; }
        self.last_ms = elapsed.as_secs_f32() * 1000.0;
        self.last = now;

        // Hot-reload
        if let Some(ref mut w) = self.watcher {
            if !w.drain().is_empty() {
                self.scene_src   = std::fs::read_to_string("assets/shaders/scene.frag").ok();
                self.overlay_src = std::fs::read_to_string("assets/shaders/overlay.frag").ok();
                log::info!("Hot-reloaded shaders");
            }
        }

        // FPS counter — update every second
        self.fps_count += 1;
        let elapsed_fps = self.fps_timer.elapsed().as_secs_f32();
        if elapsed_fps >= 1.0 {
            self.fps      = self.fps_count as f32 / elapsed_fps;
            self.fps_count = 0;
            self.fps_timer = Instant::now();
        }
        let (Some(ref mut runtime), Some(ref mut preview)) =
            (&mut self.runtime, &mut self.preview) else { return };

        // Step params and read all uniform values
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

        // Scene node — the raymarching shader + all MIDI uniforms
        let mut cfg = NodeConfig::default();
        cfg.frag_shader = self.scene_src.clone();
        cfg.uniforms    = uniforms.clone();
        configs.insert(self.node_scene, cfg);

        // Overlay node — FPS + camera bars
        let mut cfg_overlay = NodeConfig::default();
        cfg_overlay.frag_shader = self.overlay_src.clone();
        cfg_overlay.uniforms.insert("u_fps".into(),           self.fps);
        cfg_overlay.uniforms.insert("u_ms".into(),            self.last_ms);
        cfg_overlay.uniforms.insert("u_cam_angle".into(),     uniforms.get("u_cam_angle").copied().unwrap_or(0.25));
        cfg_overlay.uniforms.insert("u_cam_elevation".into(), uniforms.get("u_cam_elevation").copied().unwrap_or(0.35));
        cfg_overlay.uniforms.insert("u_cam_distance".into(),  uniforms.get("u_cam_distance").copied().unwrap_or(0.45));
        configs.insert(self.node_overlay, cfg_overlay);
        configs.insert(self.node_out, NodeConfig::default());

        let plan = match self.graph.compile() {
            Ok(p)  => p,
            Err(e) => { log::error!("Graph: {e}"); return; }
        };

        // Always render to preview
        if let Err(e) = runtime.execute_frame(&self.graph, &plan, &configs, &ctx, preview) {
            log::error!("execute_frame: {e}");
        }

        // Also push to Syphon if available
        #[cfg(target_os = "macos")]
        if let Some(ref mut so) = self.syphon_out {
            if let Err(e) = runtime.execute_frame(&self.graph, &plan, &configs, &ctx, so) {
                log::error!("syphon: {e}");
            }
        }

        // Log MIDI state every 2 seconds
        let log_every = (self.fps * LOG_INTERVAL_SEC).max(60.0) as u64;
        if self.frame % log_every == 0 {
            let angle = uniforms.get("u_cam_angle").copied().unwrap_or(0.25);
            let elev  = uniforms.get("u_cam_elevation").copied().unwrap_or(0.35);
            let dist  = uniforms.get("u_cam_distance").copied().unwrap_or(0.45);
            let speed = uniforms.get("u_speed").copied().unwrap_or(0.3);
            log::info!(
                "t={:.1}s | {:.0}fps | orbit={:.0}° elev={:.0}% dist={:.0}% speed={:.0}%",
                time,
                self.fps,
                angle * 360.0,
                elev  * 100.0,
                dist  * 100.0,
                speed * 100.0,
            );
        }

        self.frame += 1;
    }
}

impl ApplicationHandler for Raymarcher {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let win = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("scheng-raymarcher")
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
        let midi_store = Arc::clone(&self.param_store);
        self.midi = (|| -> Option<midir::MidiInputConnection<()>> {
            let mut midi_in = MidirInput::new("scheng-raymarcher").ok()?;
            midi_in.ignore(Ignore::None);
            let ports = midi_in.ports();
            if ports.is_empty() { log::warn!("MIDI: no ports found"); return None; }
            for p in &ports { if let Ok(n) = midi_in.port_name(p) { log::info!("MIDI port: '{n}'"); } }
            let port = ports.into_iter().next()?;
            let name = midi_in.port_name(&port).unwrap_or_default();
            let conn = midi_in.connect(&port, "raymarcher-in", move |_ts, msg, _| {
                if msg.len() == 3 && (msg[0] & 0xF0) == 0xB0 {
                    log::info!("[MIDI] CC{} = {}", msg[1], msg[2]);
                    midi_store.lock().unwrap().set_by_midi_cc(msg[1], msg[2]).ok();
                }
            }, ()).ok()?;
            log::info!("MIDI connected: '{name}'");
            Some(conn)
        })();

        // Syphon output
        #[cfg(target_os = "macos")]
        {
            self.syphon_out = SyphonSink::new("scheng-raymarcher")
                .map(|s| { log::info!("Syphon out: 'scheng-raymarcher' ready"); s })
                .map_err(|e| log::warn!("Syphon out: {e}")).ok();
        }

        // Load shaders
        self.scene_src   = std::fs::read_to_string("assets/shaders/scene.frag").ok();
        self.overlay_src = std::fs::read_to_string("assets/shaders/overlay.frag").ok();
        if self.scene_src.is_none()   { log::warn!("scene.frag not found — using builtin gradient"); }
        if self.overlay_src.is_none() { log::warn!("overlay.frag not found — no FPS overlay"); }

        self.watcher = AssetWatcher::new("assets").map_err(|e| log::warn!("Watcher: {e}")).ok();

        log::info!(
            "scheng-raymarcher | {}×{} | MSAA {}× | vsync uncapped | CC1–CC8 active",
            self.args.width, self.args.height, self.args.msaa
        );
        log::info!("CC1=orbit  CC2=elevation  CC3=distance  CC4=fog");
        log::info!("CC5=complexity  CC6=light-temp  CC7=reflectivity  CC8=speed");

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
    EventLoop::new().unwrap().run_app(&mut Raymarcher::new()).unwrap();
}
