//! `executor.rs` — the main runtime entry point.
//!
//! # What changed vs the original
//!
//! **Fix 3 — Ping-pong for Feedback / PreviousFrame**
//!   `WgpuRuntime` now carries `ping_pong: HashMap<NodeId, PingPong>`.
//!   Nodes of kind `Feedback` or `PreviousFrame` get a pair of render targets.
//!   Each frame: render into `current`, bind `previous` as iChannel0, then swap.
//!   This makes `NodeKind::PreviousFrame` and `NodeKind::Feedback` actually work.
//!
//! **Fix 4 — Geometry pipeline (MSH3)**
//!   `WgpuRuntime` now carries `vertex_buffers: HashMap<NodeId, GeometryBuffers>`.
//!   When `NodeConfig::topology != Fullscreen`:
//!   - A vertex buffer is created/updated from `config.vertex_data`.
//!   - `PipelineCache::get_or_create` receives the topology so a separate
//!     geometry pipeline is compiled and cached.
//!   - `MvpBlock` is uploaded per-node per-frame (identity for fullscreen nodes).
//!   - `draw()` passes the vertex count from the buffer rather than 3.

use std::collections::HashMap;


use scheng_graph::{Graph, NodeId, NodeKind, Plan};

use wgpu::util::DeviceExt;   // needed for create_buffer_init

use scheng_param_store::{
    NodeConfig,
};

use crate::{
    context::WgpuContext,
    pipeline::PipelineCache,
    render_target::{create_blank_texture, RenderTarget},
    uniforms::{CustomUniformBuffer, MvpUniformBuffer, UniformManager},
    FrameCtx, WgpuError,
};

// ── OutputSink ────────────────────────────────────────────────────────────

pub trait OutputSink {
    fn present(
        &mut self,
        node_id: NodeId,
        target:  &RenderTarget,
        ctx:     &FrameCtx,
        device:  &wgpu::Device,
        queue:   &wgpu::Queue,
    );
}

// ── PingPong pair ─────────────────────────────────────────────────────────

/// Two render targets for Feedback / PreviousFrame nodes.
///
/// - `current`: rendered into this frame.
/// - `previous`: bound as iChannel0 (the output of the *previous* frame).
///
/// After the frame is rendered, call `swap()`. On the next frame,
/// `previous` contains last frame's output, ready to be sampled.
struct PingPong {
    current:  RenderTarget,
    previous: RenderTarget,
}

impl PingPong {
    fn new(device: &wgpu::Device, width: u32, height: u32, label: &str) -> Self {
        Self {
            current:  RenderTarget::new(device, width, height, &format!("{label}_cur")),
            previous: RenderTarget::new(device, width, height, &format!("{label}_prev")),
        }
    }

    fn ensure_size(&mut self, device: &wgpu::Device, width: u32, height: u32,
                   sample_count: u32, label: &str) {
        self.current.ensure_size_msaa(device, width, height, sample_count,
                                       &format!("{label}_cur"));
        self.previous.ensure_size_msaa(device, width, height, sample_count,
                                        &format!("{label}_prev"));
    }

    fn swap(&mut self) {
        std::mem::swap(&mut self.current, &mut self.previous);
    }
}

// ── GeometryBuffers ───────────────────────────────────────────────────────

/// Vertex buffer state for geometry nodes.
struct GeometryBuffers {
    vertex_buffer: wgpu::Buffer,
    vertex_count:  u32,
    /// Cached length used to detect when the buffer needs reallocation.
    capacity:      u32,
}

impl GeometryBuffers {
    fn new(device: &wgpu::Device, data: &[[f32; 2]], label: &str) -> Self {
        let bytes = bytemuck_cast_slice(data);
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some(label),
            contents: bytes,
            usage:    wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            vertex_buffer: buffer,
            vertex_count:  data.len() as u32,
            capacity:      data.len() as u32,
        }
    }

    /// Update vertex data. Reallocates if the new data is larger than the buffer.
    fn update(
        &mut self,
        device: &wgpu::Device,
        queue:  &wgpu::Queue,
        data:   &[[f32; 2]],
        label:  &str,
    ) {
        let len = data.len() as u32;
        if len > self.capacity {
            // Realloc — grow the buffer
            *self = Self::new(device, data, label);
        } else {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck_cast_slice(data));
            self.vertex_count = len;
        }
    }
}

/// Cast `&[[f32; 2]]` to `&[u8]` without depending on bytemuck directly here.
fn bytemuck_cast_slice(data: &[[f32; 2]]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            data.as_ptr() as *const u8,
            data.len() * std::mem::size_of::<[f32; 2]>(),
        )
    }
}




// ── Built-in shaders ──────────────────────────────────────────────────────

const BUILTIN_FRAG_SOURCE: &str = r#"
void main() {
    float r = v_uv.x + 0.5 * sin(uTime);
    float g = v_uv.y + 0.5 * cos(uTime * 0.7);
    fragColor = vec4(r, g, 0.2, 1.0);
}
"#;

const BUILTIN_FRAG_PASSTHROUGH: &str = r#"
void main() {
    fragColor = texture(iChannel0, v_uv);
}
"#;

/// Crossfade with parametric u_mix (default 0.5 if not supplied).
/// u_mix = 0.0 → full A, u_mix = 1.0 → full B.
const BUILTIN_FRAG_CROSSFADE: &str = r#"
uniform float u_mix;
void main() {
    vec4 a = texture(iChannel0, v_uv);
    vec4 b = texture(iChannel1, v_uv);
    fragColor = mix(a, b, u_mix);
}
"#;

const BUILTIN_FRAG_ADD: &str = r#"
void main() {
    fragColor = clamp(
        texture(iChannel0, v_uv) + texture(iChannel1, v_uv),
        0.0, 1.0
    );
}
"#;

const BUILTIN_FRAG_MULTIPLY: &str = r#"
void main() {
    fragColor = texture(iChannel0, v_uv) * texture(iChannel1, v_uv);
}
"#;

/// Feedback: sample iChannel0 (previous frame) + optional decay.
const BUILTIN_FRAG_FEEDBACK: &str = r#"
uniform float u_decay;
void main() {
    vec4 prev = texture(iChannel0, v_uv);
    // decay = 1.0 → perfect loop (no decay). decay = 0.0 → instant black.
    fragColor = prev * u_decay;
}
"#;

/// Default geometry (MSH3 placeholder): white line on black.
const BUILTIN_FRAG_GEOMETRY: &str = r#"
void main() {
    fragColor = vec4(1.0, 1.0, 1.0, 1.0);
}
"#;

/// MatrixMix4: weighted sum of 4 inputs. Weights via u_w0..u_w3.
const BUILTIN_FRAG_MATRIX4: &str = r#"
uniform float u_w0;
uniform float u_w1;
uniform float u_w2;
uniform float u_w3;
void main() {
    fragColor =
        texture(iChannel0, v_uv) * u_w0 +
        texture(iChannel1, v_uv) * u_w1 +
        texture(iChannel2, v_uv) * u_w2 +
        texture(iChannel3, v_uv) * u_w3;
}
"#;

fn builtin_frag(kind: &NodeKind) -> Option<&'static str> {
    match kind {
        NodeKind::ShaderSource
        | NodeKind::NoiseSource
        | NodeKind::TextureInputPass
        | NodeKind::VideoDecodeSource => Some(BUILTIN_FRAG_SOURCE),

        // PreviousFrame: passthrough of iChannel0 (which is the previous render target).
        // The ping-pong logic in the executor binds the previous RenderTarget as iChannel0.
        NodeKind::PreviousFrame => Some(BUILTIN_FRAG_PASSTHROUGH),

        NodeKind::ShaderPass
        | NodeKind::ColorCorrect
        | NodeKind::Blur
        | NodeKind::Keyer => Some(BUILTIN_FRAG_PASSTHROUGH),

        // Feedback: samples previous frame (iChannel0 = ping-pong previous target).
        NodeKind::Feedback => Some(BUILTIN_FRAG_FEEDBACK),

        NodeKind::Crossfade
        | NodeKind::KeyMix
        | NodeKind::ShaderMix2 => Some(BUILTIN_FRAG_CROSSFADE),

        NodeKind::ShaderMix3
        | NodeKind::ShaderMix4 => Some(BUILTIN_FRAG_CROSSFADE), // TODO: 3/4-way blend

        NodeKind::MatrixMix4 => Some(BUILTIN_FRAG_MATRIX4),

        NodeKind::Add      => Some(BUILTIN_FRAG_ADD),
        NodeKind::Multiply => Some(BUILTIN_FRAG_MULTIPLY),

        NodeKind::Window
        | NodeKind::TextureOut
        | NodeKind::PixelsOut
        | NodeKind::Syphon
        | NodeKind::Spout
        | NodeKind::Recorder
        | NodeKind::Ndi
        | NodeKind::Rtsp => None,
    }
}

fn is_output_kind(kind: &NodeKind) -> bool {
    builtin_frag(kind).is_none()
}

fn is_pingpong_kind(kind: &NodeKind) -> bool {
    matches!(kind, NodeKind::PreviousFrame | NodeKind::Feedback)
}

/// Port name → iChannel index.
fn port_name_to_channel(name: &str) -> Option<usize> {
    match name {
        "in" | "in0" | "a" | "src"  => Some(0),
        "in1" | "b"  | "src1"       => Some(1),
        "in2" | "c"  | "src2"       => Some(2),
        "in3" | "d"  | "src3"       => Some(3),
        _ => None,
    }
}

// ── Free functions (avoid &self borrow conflicts) ─────────────────────────

fn resolve_inputs(
    graph:          &Graph,
    plan:           &Plan,
    node_id:        NodeId,
    render_targets: &HashMap<NodeId, RenderTarget>,
    ping_pong:      &HashMap<NodeId, PingPong>,
    blank_texture:  &wgpu::Texture,
    config:         &NodeConfig,
) -> [wgpu::TextureView; 4] {
    let blank = || blank_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut views: [Option<wgpu::TextureView>; 4] = [None, None, None, None];

    // 1. External overrides (webcam, NDI, etc.) — highest priority
    for (ch, slot) in config.input_textures.iter().enumerate() {
        if let Some(tex) = slot {
            views[ch] = Some(tex.create_view(&wgpu::TextureViewDescriptor::default()));
        }
    }

    // 2. Graph edges
    for edge in plan.edges.iter().filter(|e| e.to.node == node_id) {
        let channel = graph.node(node_id).and_then(|n| {
            n.ports.iter()
                .find(|p| p.id == edge.to.port)
                .and_then(|p| port_name_to_channel(&p.name))
        });
        if let Some(ch) = channel {
            if views[ch].is_none() {
                // For ping-pong nodes: bind the *previous* frame as iChannel0
                if let Some(pp) = ping_pong.get(&edge.from.node) {
                    if ch == 0 {
                        views[0] = Some(
                            pp.previous.texture.create_view(
                                &wgpu::TextureViewDescriptor::default()
                            )
                        );
                        continue;
                    }
                }
                if let Some(t) = render_targets.get(&edge.from.node) {
                    views[ch] = Some(
                        t.texture.create_view(&wgpu::TextureViewDescriptor::default())
                    );
                }
            }
        }
    }

    [
        views[0].take().unwrap_or_else(blank),
        views[1].take().unwrap_or_else(blank),
        views[2].take().unwrap_or_else(blank),
        views[3].take().unwrap_or_else(blank),
    ]
}

fn build_bind_group(
    device:          &wgpu::Device,
    layout:          &wgpu::BindGroupLayout,
    views:           &[wgpu::TextureView; 4],
    sampler:         &wgpu::Sampler,
    uniform_manager: &UniformManager,
    custom_buffer:   &CustomUniformBuffer,
    mvp_buffer:      &MvpUniformBuffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label:   Some("scheng_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&views[0]) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&views[1]) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&views[2]) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&views[3]) },
            wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(sampler) },
            wgpu::BindGroupEntry { binding: 5, resource: uniform_manager.buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 6, resource: custom_buffer.buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 7, resource: mvp_buffer.buffer.as_entire_binding() },
        ],
    })
}

// ── WgpuRuntime ───────────────────────────────────────────────────────────

pub struct WgpuRuntime {
    pub ctx:         WgpuContext,
    pub pipelines:   PipelineCache,
    render_targets:  HashMap<NodeId, RenderTarget>,
    /// Fix 3: ping-pong targets for Feedback / PreviousFrame nodes.
    ping_pong:       HashMap<NodeId, PingPong>,
    uniform_manager:        UniformManager,
    custom_uniform_buffers: HashMap<NodeId, CustomUniformBuffer>,
    /// Fix 4: per-node MVP buffers (identity for fullscreen nodes).
    mvp_buffers:     HashMap<NodeId, MvpUniformBuffer>,
    /// Fix 4: vertex buffers for geometry nodes.
    geometry_buffers: HashMap<NodeId, GeometryBuffers>,
    blank_texture:   wgpu::Texture,
    sampler:         wgpu::Sampler,
}

impl WgpuRuntime {
    pub fn new(width: u32, height: u32) -> Result<Self, WgpuError> {
        let ctx             = WgpuContext::new()?;
        let uniform_manager = UniformManager::new(&ctx.device);
        let blank_texture   = create_blank_texture(&ctx.device, &ctx.queue);
        let sampler         = create_sampler(&ctx.device);
        log::info!(
            "scheng-runtime-wgpu ready — {}×{} — {}",
            width, height, ctx.adapter_info.name
        );
        Ok(Self {
            ctx,
            pipelines:              PipelineCache::new(),
            render_targets:         HashMap::new(),
            ping_pong:              HashMap::new(),
            uniform_manager,
            custom_uniform_buffers: HashMap::new(),
            mvp_buffers:            HashMap::new(),
            geometry_buffers:       HashMap::new(),
            blank_texture,
            sampler,
        })
    }

    /// Initialise from a pre-existing WgpuContext.
    ///
    /// Use when the context was created with surface compatibility via
    /// `WgpuContext::new_with_surface()`. The same device/queue are reused —
    /// no second GPU device is allocated.
    pub fn from_context(ctx: WgpuContext, width: u32, height: u32) -> Result<Self, WgpuError> {
        let uniform_manager = UniformManager::new(&ctx.device);
        let blank_texture   = create_blank_texture(&ctx.device, &ctx.queue);
        let sampler         = create_sampler(&ctx.device);
        log::info!(
            "scheng-runtime-wgpu ready (windowed) — {}×{} — {}",
            width, height, ctx.adapter_info.name
        );
        Ok(Self {
            ctx,
            pipelines:              PipelineCache::new(),
            render_targets:         HashMap::new(),
            ping_pong:              HashMap::new(),
            uniform_manager,
            custom_uniform_buffers: HashMap::new(),
            mvp_buffers:            HashMap::new(),
            geometry_buffers:       HashMap::new(),
            blank_texture,
            sampler,
        })
    }

    pub fn execute_frame(
        &mut self,
        graph:   &Graph,
        plan:    &Plan,
        configs: &HashMap<NodeId, NodeConfig>,
        ctx:     &FrameCtx,
        sink:    &mut dyn OutputSink,
    ) -> Result<(), WgpuError> {
        self.uniform_manager.update(&self.ctx.queue, ctx);

        // ── Phase A: create/resize render targets ──────────────────────────
        for &node_id in &plan.nodes {
            let node = graph.node(node_id).ok_or_else(||
                WgpuError::Wgpu(format!("Plan references unknown NodeId {node_id:?}"))
            )?;
            if is_output_kind(&node.kind) { continue; }

            let label = format!("{:?}_{node_id:?}", node.kind);

            if is_pingpong_kind(&node.kind) {
                // Fix 3: ping-pong nodes get a pair of render targets.
                let pp = self.ping_pong
                    .entry(node_id)
                    .or_insert_with(|| PingPong::new(&self.ctx.device, ctx.width, ctx.height, &label));
                pp.ensure_size(&self.ctx.device, ctx.width, ctx.height, ctx.sample_count, &label);
            } else {
                let target = self.render_targets
                    .entry(node_id)
                    .or_insert_with(|| RenderTarget::new(&self.ctx.device, ctx.width, ctx.height, &label));
                target.ensure_size_msaa(&self.ctx.device, ctx.width, ctx.height, ctx.sample_count, &label);
            }
        }

        // ── Phase B: encode render passes ──────────────────────────────────
        let mut encoder = self.ctx.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("scheng_frame") }
        );

        for &node_id in &plan.nodes {
            let node = graph.node(node_id).ok_or_else(||
                WgpuError::Wgpu(format!("Plan references unknown NodeId {node_id:?}"))
            )?;
            let kind = &node.kind;
            if is_output_kind(kind) { continue; }

            let config = configs.get(&node_id)
                .ok_or(WgpuError::MissingNodeConfig(node_id))?;

            let frag_src = config.frag_shader.as_deref()
                .or_else(|| builtin_frag(kind))
                .unwrap_or(BUILTIN_FRAG_SOURCE);

            let topology = config.topology;
            let label    = format!("{kind:?}_{node_id:?}");

            // ── Fix 4: update geometry vertex buffer if needed ─────────────
            if topology.is_geometry() {
                if let Some(vdata) = &config.vertex_data {
                    let geo = self.geometry_buffers
                        .entry(node_id)
                        .or_insert_with(|| GeometryBuffers::new(&self.ctx.device, vdata, &label));
                    geo.update(&self.ctx.device, &self.ctx.queue, vdata, &label);
                }
            }

            // ── Resolve inputs ─────────────────────────────────────────────
            let input_views = resolve_inputs(
                graph, plan, node_id,
                &self.render_targets,
                &self.ping_pong,
                &self.blank_texture,
                config,
            );

            // ── Get/create pipeline (Fix 4: pass topology) ─────────────────
            let node_pipeline = self.pipelines.get_or_create(
                &self.ctx.device, frag_src, &label, ctx.sample_count, topology
            )?;

            // ── Custom uniforms ────────────────────────────────────────────
            let custom_buf = self.custom_uniform_buffers
                .entry(node_id)
                .or_insert_with(|| CustomUniformBuffer::new(&self.ctx.device, &label));
            custom_buf.update(&self.ctx.queue, &node_pipeline.custom_uniform_names, &config.uniforms);

            // ── Fix 4: MVP uniform (identity for fullscreen) ───────────────
            let mvp_buf = self.mvp_buffers
                .entry(node_id)
                .or_insert_with(|| MvpUniformBuffer::new(&self.ctx.device, &label));
            mvp_buf.update(&self.ctx.queue, config.mvp);

            // ── Bind group ─────────────────────────────────────────────────
            let bind_group = build_bind_group(
                &self.ctx.device,
                &node_pipeline.bind_group_layout,
                &input_views,
                &self.sampler,
                &self.uniform_manager,
                custom_buf,
                mvp_buf,
            );

            // ── Choose render target ───────────────────────────────────────
            // Fix 3: ping-pong nodes render into `current`.
            let (attachment_ptr, resolve_ptr) = if is_pingpong_kind(kind) {
                let pp = self.ping_pong.get(&node_id)
                    .expect("ping-pong target missing after Phase A");
                get_attachment_ptrs(&pp.current)
            } else {
                let rt = self.render_targets.get(&node_id)
                    .expect("render target missing after Phase A");
                get_attachment_ptrs(rt)
            };

            // ── Record render pass ─────────────────────────────────────────
            {
                let attachment    = unsafe { &*attachment_ptr };
                let resolve_target = resolve_ptr.map(|p| unsafe { &*p });

                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(&label),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view:           attachment,
                        resolve_target,
                        ops: wgpu::Operations {
                            load:  wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes:         None,
                    occlusion_query_set:      None,
                });

                pass.set_pipeline(&node_pipeline.pipeline);
                pass.set_bind_group(0, &bind_group, &[]);

                // Fix 4: set vertex buffer for geometry, draw 3 vertices for fullscreen
                if topology.is_geometry() {
                    if let Some(geo) = self.geometry_buffers.get(&node_id) {
                        pass.set_vertex_buffer(0, geo.vertex_buffer.slice(..));
                        pass.draw(0..geo.vertex_count, 0..1);
                    }
                    // If no vertex data yet, draw nothing (node exists but has no geometry)
                } else {
                    pass.draw(0..3, 0..1);  // fullscreen triangle
                }
            }

            log::trace!("Rendered {label} ({topology:?})");
        }

        self.ctx.queue.submit(std::iter::once(encoder.finish()));

        // ── Fix 3: swap ping-pong targets after the frame is submitted ─────
        for pp in self.ping_pong.values_mut() {
            pp.swap();
        }

        // ── Phase C: present output nodes ──────────────────────────────────
        for &node_id in &plan.nodes {
            let node = graph.node(node_id).ok_or_else(||
                WgpuError::Wgpu(format!("Plan references unknown NodeId {node_id:?}"))
            )?;
            if !is_output_kind(&node.kind) { continue; }

            let upstream_id = plan.edges.iter()
                .find(|e| e.to.node == node_id)
                .map(|e| e.from.node);

            // Output might connect from a ping-pong node's current target
            let target = upstream_id.and_then(|id| {
                if let Some(pp) = self.ping_pong.get(&id) {
                    // After swap(), the *just-rendered* frame is now in `previous`
                    Some(&pp.previous)
                } else {
                    self.render_targets.get(&id)
                }
            }).ok_or(WgpuError::NoRenderTarget(node_id))?;

            sink.present(node_id, target, ctx, &self.ctx.device, &self.ctx.queue);
        }

        Ok(())
    }

    pub fn clear_pipeline_cache(&mut self) {
        self.pipelines.clear();
    }

    pub fn readback_pixels(&self, node_id: NodeId) -> Result<Vec<u8>, WgpuError> {
        // Check regular targets first, then ping-pong
        if let Some(target) = self.render_targets.get(&node_id) {
            return Ok(target.readback(&self.ctx.device, &self.ctx.queue));
        }
        if let Some(pp) = self.ping_pong.get(&node_id) {
            return Ok(pp.previous.readback(&self.ctx.device, &self.ctx.queue));
        }
        Err(WgpuError::NoRenderTarget(node_id))
    }
}

/// Returns raw pointers to attachment and resolve views from a RenderTarget.
/// SAFETY: caller must not mutably borrow the render target while these pointers are live.
fn get_attachment_ptrs(rt: &RenderTarget)
    -> (*const wgpu::TextureView, Option<*const wgpu::TextureView>)
{
    let att = if rt.msaa_view.is_some() {
        rt.msaa_view.as_ref().unwrap() as *const wgpu::TextureView
    } else {
        &rt.render_view as *const wgpu::TextureView
    };
    let res = if rt.msaa_view.is_some() {
        Some(&rt.render_view as *const wgpu::TextureView)
    } else {
        None
    };
    (att, res)
}

fn create_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label:          Some("scheng_sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter:     wgpu::FilterMode::Linear,
        min_filter:     wgpu::FilterMode::Linear,
        mipmap_filter:  wgpu::FilterMode::Nearest,
        ..Default::default()
    })
}

// ── PixelReadbackSink ─────────────────────────────────────────────────────

pub struct PixelReadbackSink {
    pixels: HashMap<NodeId, Vec<u8>>,
}

impl PixelReadbackSink {
    pub fn new() -> Self { Self { pixels: HashMap::new() } }

    pub fn take_pixels(&mut self, node_id: NodeId) -> Option<Vec<u8>> {
        self.pixels.remove(&node_id)
    }

    pub fn pixels(&self, node_id: NodeId) -> Option<&[u8]> {
        self.pixels.get(&node_id).map(|v| v.as_slice())
    }
}

impl OutputSink for PixelReadbackSink {
    fn present(
        &mut self,
        node_id: NodeId,
        target:  &RenderTarget,
        _ctx:    &FrameCtx,
        device:  &wgpu::Device,
        queue:   &wgpu::Queue,
    ) {
        self.pixels.insert(node_id, target.readback(device, queue));
    }
}


