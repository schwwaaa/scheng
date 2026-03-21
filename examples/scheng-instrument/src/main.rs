//! # scheng instrument example
//!
//! Complete instrument with preview window, hot-reload, MIDI/OSC,
//! Syphon, NDI, and FFmpeg output.
//!
//! ```bash
//! cargo run --release --no-default-features          # preview only
//! cargo run --release                                # preview + Syphon
//! cargo run --release --features ndi                 # + NDI output
//! cargo run --release -- --stream rtsp://localhost:8554/live
//! cargo run --release -- --record recording.mp4
//! cargo run --release --features ndi -- --ndi-name "my source"
//! ```

mod preview;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

use scheng_graph::{Graph, NodeId, NodeKind};
use scheng_runtime_wgpu::{executor::OutputSink, FrameCtx, WgpuRuntime};
use scheng_param_store::{NodeConfigBuilder, ParamStore};
use scheng_control_osc_wgpu::OscReceiver;
use scheng_hotreload::HotReloader;
use scheng_output_ffmpeg::{FfmpegConfig, FfmpegSink, config::OutputTarget};
use preview::PreviewSink;

#[cfg(feature = "midi")]
use scheng_input_midi::MidiInput;

#[cfg(all(target_os = "macos", feature = "syphon"))]
use scheng_output_syphon::SyphonSink;

#[cfg(feature = "ndi")]
use scheng_output_ndi::{NdiSink, NdiConfig};

const ASSETS_DIR:     &str = "assets";
const SHADER_PATH:    &str = "assets/shaders/main.frag";
const PARAMS_PATH:    &str = "assets/params.json";
const TARGET_FPS:     u32  = 30;
const SYPHON_NAME:    &str = "scheng";
const NDI_NAME:       &str = "scheng";
const DEFAULT_WIDTH:  u32  = 1280;
const DEFAULT_HEIGHT: u32  = 720;

fn frame_budget() -> Duration { Duration::from_nanos(1_000_000_000 / TARGET_FPS as u64) }

// ── Args ──────────────────────────────────────────────────────────────────

struct Args {
    width:      u32,
    height:     u32,
    stream_url: Option<String>,
    record:     Option<String>,
    osc_port:   u16,
    ndi_name:   String,
}

impl Args {
    fn parse() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let mut a = Args {
            width: DEFAULT_WIDTH, height: DEFAULT_HEIGHT,
            stream_url: None, record: None,
            osc_port: 9000,
            ndi_name: NDI_NAME.to_string(),
        };
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--width"    => { i+=1; a.width      = args[i].parse().unwrap_or(DEFAULT_WIDTH); }
                "--height"   => { i+=1; a.height     = args[i].parse().unwrap_or(DEFAULT_HEIGHT); }
                "--stream"   => { i+=1; a.stream_url = Some(args[i].clone()); }
                "--record"   => { i+=1; a.record     = Some(args[i].clone()); }
                "--osc-port" => { i+=1; a.osc_port   = args[i].parse().unwrap_or(9000); }
                "--ndi-name" => { i+=1; a.ndi_name   = args[i].clone(); }
                _ => {}
            }
            i += 1;
        }
        a
    }
}

// ── Instrument ────────────────────────────────────────────────────────────

struct Instrument {
    args:      Args,
    window:    Option<Arc<Window>>,
    runtime:   Option<WgpuRuntime>,
    graph:     Option<Graph>,
    plan:      Option<scheng_graph::Plan>,
    main_node: Option<NodeId>,
    out_node:  Option<NodeId>,
    store:     Arc<Mutex<ParamStore>>,
    builder:   NodeConfigBuilder,
    osc:       Option<OscReceiver>,
    reloader:  Option<HotReloader>,
    preview:   Option<PreviewSink>,
    ffmpeg:    Option<FfmpegSink>,
    #[cfg(all(target_os = "macos", feature = "syphon"))]
    syphon:    Option<SyphonSink>,
    #[cfg(feature = "ndi")]
    ndi:       Option<NdiSink>,
    #[cfg(feature = "midi")]
    _midi:     Option<MidiInput>,
    start:     Instant,
    frame:     u64,
}

impl Instrument {
    fn new(args: Args) -> Self {
        let store = Arc::new(Mutex::new(
            ParamStore::from_json_file(PARAMS_PATH).unwrap_or_else(|e| {
                log::warn!("No {PARAMS_PATH}: {e}"); ParamStore::empty()
            })
        ));

        #[cfg(feature = "midi")]
        let _midi = MidiInput::connect_first(Arc::clone(&store))
            .map(|m| { log::info!("MIDI: {}", m.port_name()); m })
            .map_err(|e| log::warn!("MIDI: {e}")).ok();

        let osc = OscReceiver::bind(&format!("127.0.0.1:{}", args.osc_port))
            .map(|r| { log::info!("OSC: 127.0.0.1:{}", args.osc_port); r })
            .map_err(|e| log::warn!("OSC: {e}")).ok();

        let reloader = HotReloader::new(ASSETS_DIR)
            .map(|r| { log::info!("Hot-reload: {ASSETS_DIR}/"); r })
            .map_err(|e| log::warn!("Hot-reload: {e}")).ok();

        Self {
            args, window: None, runtime: None, graph: None, plan: None,
            main_node: None, out_node: None,
            store, builder: NodeConfigBuilder::new(), osc, reloader,
            preview: None, ffmpeg: None,
            #[cfg(all(target_os = "macos", feature = "syphon"))]
            syphon: None,
            #[cfg(feature = "ndi")]
            ndi: None,
            #[cfg(feature = "midi")]
            _midi,
            start: Instant::now(), frame: 0,
        }
    }

    fn build_graph(&mut self) {
        let mut g = Graph::new();
        let main  = g.add_node(NodeKind::ShaderSource);
        let out   = g.add_node(NodeKind::PixelsOut);
        g.connect_named(main, "out", out, "in").unwrap();
        let plan  = g.compile().unwrap();
        self.main_node = Some(main);
        self.out_node  = Some(out);
        self.graph     = Some(g);
        self.plan      = Some(plan);
        self.builder   = NodeConfigBuilder::new();
        self.builder.register("main", main);
        let shader = std::fs::read_to_string(SHADER_PATH).unwrap_or_else(|_| {
            log::warn!("Shader {SHADER_PATH} not found — built-in gradient"); String::new()
        });
        if !shader.is_empty() { self.builder.set_shader(main, shader); }
        if let Some(ref mut r) = self.reloader { r.register_shader(SHADER_PATH, main); }
    }

    fn tick(&mut self) {
        let graph = match &self.graph { Some(g) => g as *const _, None => return };
        let plan  = match &self.plan  { Some(p) => p as *const _, None => return };
        let graph = unsafe { &*graph };
        let plan  = unsafe { &*plan };

        if let Some(ref mut r) = self.osc {
            let mut s = self.store.lock().unwrap();
            r.poll(&mut *s);
        }
        if let Some(ref mut r) = self.reloader {
            let mut s = self.store.lock().unwrap();
            let n = r.check(&mut self.builder, &mut *s);
            if n > 0 { log::info!("Hot-reloaded {n} file(s)"); }
        }
        self.store.lock().unwrap().step_frame();

        let configs = { let s = self.store.lock().unwrap(); self.builder.build(&*s) };
        let ctx = FrameCtx {
            width: self.args.width, height: self.args.height,
            time: self.start.elapsed().as_secs_f32(), frame: self.frame,
        };

        let runtime = match &mut self.runtime { Some(r) => r, None => return };
        let mut multi = MultiSink::default();
        if let Some(ref mut s) = self.preview { multi.add(s); }
        if let Some(ref mut s) = self.ffmpeg  { multi.add(s); }
        #[cfg(all(target_os = "macos", feature = "syphon"))]
        if let Some(ref mut s) = self.syphon  { multi.add(s); }
        #[cfg(feature = "ndi")]
        if let Some(ref mut s) = self.ndi     { multi.add(s); }

        if let Err(e) = runtime.execute_frame(graph, plan, &configs, &ctx, &mut multi) {
            log::error!("execute_frame: {e}");
        }

        self.frame += 1;
        if self.frame % (TARGET_FPS as u64 * 10) == 0 {
            log::info!("Frame {} | t={:.1}s | reloads={}", self.frame, ctx.time,
                self.reloader.as_ref().map(|r| r.reload_count()).unwrap_or(0));
        }
    }
}

// ── winit event handler ───────────────────────────────────────────────────

impl ApplicationHandler for Instrument {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() { return; }

        let win = Arc::new(event_loop.create_window(
            WindowAttributes::default()
                .with_title("scheng")
                .with_inner_size(winit::dpi::LogicalSize::new(self.args.width, self.args.height))
        ).expect("Window creation failed"));

        let win_ref: &'static Window = unsafe { &*(Arc::as_ptr(&win)) };

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY, ..Default::default()
        });
        let surface = instance.create_surface(win_ref)
            .expect("Surface creation failed");

        self.build_graph();

        let runtime = WgpuRuntime::new_with_surface(instance, &surface, self.args.width, self.args.height)
            .expect("WgpuRuntime failed");
        log::info!("GPU: {}", runtime.ctx.adapter_info.name);

        let preview = PreviewSink::new(
            surface,
            &runtime.ctx.device,
            &runtime.ctx.queue,
            &runtime.ctx.adapter,
            self.out_node.unwrap(),
            self.args.width,
            self.args.height,
        );

        // Syphon
        #[cfg(all(target_os = "macos", feature = "syphon"))]
        {
            self.syphon = SyphonSink::new(SYPHON_NAME)
                .map(|s| { log::info!("Syphon: '{}' ready", SYPHON_NAME); s })
                .map_err(|e| log::warn!("Syphon: {e}")).ok();
        }

        // NDI
        #[cfg(feature = "ndi")]
        {
            self.ndi = NdiSink::new(NdiConfig {
                source_name:   self.args.ndi_name.clone(),
                group:         None,
                framerate_num: TARGET_FPS,
                framerate_den: 1,
            })
            .map(|s| { log::info!("NDI: '{}' ready", self.args.ndi_name); s })
            .map_err(|e| log::warn!("NDI: {e}")).ok();
        }

        // FFmpeg
        self.ffmpeg = if let Some(url) = &self.args.stream_url {
            log::info!("FFmpeg: → {url}");
            FfmpegSink::new(FfmpegConfig {
                width: self.args.width, height: self.args.height, framerate: TARGET_FPS,
                target: OutputTarget::Rtsp { url: url.clone() }, ..Default::default()
            }).map_err(|e| log::error!("FFmpeg: {e}")).ok()
        } else if let Some(path) = &self.args.record {
            log::info!("FFmpeg: → {path}");
            FfmpegSink::new(FfmpegConfig {
                width: self.args.width, height: self.args.height, framerate: TARGET_FPS,
                target: OutputTarget::File { path: path.clone(), overwrite: true }, ..Default::default()
            }).map_err(|e| log::error!("FFmpeg: {e}")).ok()
        } else { None };

        self.preview = Some(preview);
        self.runtime = Some(runtime);
        self.window  = Some(win);
        self.start   = Instant::now();

        log::info!("{}×{} @ {}fps | edit {SHADER_PATH} to hot-reload",
            self.args.width, self.args.height, TARGET_FPS);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(sz) => {
                if let (Some(ref mut p), Some(ref r)) = (&mut self.preview, &self.runtime) {
                    p.resize(&r.ctx.device, sz.width, sz.height);
                }
            }
            WindowEvent::RedrawRequested => {
                let t0 = Instant::now();
                self.tick();
                let elapsed = t0.elapsed();
                if elapsed < frame_budget() { std::thread::sleep(frame_budget() - elapsed); }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(ref w) = self.window { w.request_redraw(); }
    }
}

// ── MultiSink ─────────────────────────────────────────────────────────────

#[derive(Default)]
struct MultiSink<'a> { sinks: Vec<&'a mut dyn OutputSink> }

impl<'a> MultiSink<'a> {
    fn add(&mut self, s: &'a mut dyn OutputSink) { self.sinks.push(s); }
}

impl<'a> OutputSink for MultiSink<'a> {
    fn present(&mut self, n: NodeId, t: &scheng_runtime_wgpu::RenderTarget,
               c: &FrameCtx, d: &wgpu::Device, q: &wgpu::Queue) {
        for s in &mut self.sinks { s.present(n, t, c, d, q); }
    }
}

fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();
    let event_loop = EventLoop::new().expect("EventLoop failed");
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut Instrument::new(Args::parse())).expect("Event loop error");
}
