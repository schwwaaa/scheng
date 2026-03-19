//! `executor.rs` — WgpuRuntime: execute_frame, OutputSink, NodeConfig.

use std::collections::HashMap;

use scheng_graph::{Graph, NodeId, NodeKind, Plan};

use crate::{
    context::WgpuContext,
    pipeline::PipelineCache,
    render_target::{create_blank_texture, PingPongTarget, RenderTarget},
    uniforms::{CustomUniformBuffer, UniformManager},
    FrameCtx, WgpuError,
};

// ── OutputSink ────────────────────────────────────────────────────────────

/// Consume the rendered output of an Output node each frame.
pub trait OutputSink {
    /// Called once per Output node after all GPU work is submitted.
    fn present(
        &mut self,
        node_id: NodeId,
        target:  &RenderTarget,
        ctx:     &FrameCtx,
        device:  &wgpu::Device,
        queue:   &wgpu::Queue,
    );
}

// ── NodeConfig ────────────────────────────────────────────────────────────

/// Per-node configuration supplied by the instrument each frame.
pub struct NodeConfig {
    /// GLSL 330 fragment shader. `None` → use built-in for this NodeKind.
    pub frag_shader: Option<String>,
    /// Custom uniform values — maps u_* name → f32 value.
    pub uniforms: HashMap<String, f32>,
    /// Output name for PixelsOut nodes. `None` = primary output.
    pub output_name: Option<String>,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self { frag_shader: None, uniforms: HashMap::new(), output_name: None }
    }
}

impl NodeConfig {
    pub fn set(&mut self, name: &str, value: f32) -> &mut Self {
        self.uniforms.insert(name.to_owned(), value);
        self
    }
}

// ── Built-in shaders ──────────────────────────────────────────────────────

const BUILTIN_SOURCE: &str = r#"
void main() {
    float r = v_uv.x + 0.5 * sin(uTime);
    float g = v_uv.y + 0.5 * cos(uTime * 0.7);
    fragColor = vec4(r, g, 0.2, 1.0);
}
"#;
const BUILTIN_PASS:  &str = "void main() { fragColor = texture(iChannel0, v_uv); }";
const BUILTIN_XFADE: &str = "void main() { fragColor = mix(texture(iChannel0, v_uv), texture(iChannel1, v_uv), 0.5); }";
const BUILTIN_ADD:   &str = "void main() { fragColor = clamp(texture(iChannel0, v_uv) + texture(iChannel1, v_uv), 0.0, 1.0); }";
const BUILTIN_MUL:   &str = "void main() { fragColor = texture(iChannel0, v_uv) * texture(iChannel1, v_uv); }";
/// Default feedback: blend live input with decayed previous frame
const BUILTIN_FEEDBACK: &str = r#"
void main() {
    vec4 live     = texture(iChannel0, v_uv);
    vec4 previous = texture(iChannel1, v_uv);
    fragColor = clamp(live + previous * 0.85, 0.0, 1.0);
}
"#;

fn builtin_frag(kind: &NodeKind) -> Option<&'static str> {
    match kind {
        NodeKind::ShaderSource | NodeKind::NoiseSource
        | NodeKind::TextureInputPass
        | NodeKind::VideoDecodeSource  => Some(BUILTIN_SOURCE),

        // PreviousFrame: passes iChannel0 through — the ping-pong read texture
        // will be injected as iChannel0 by the executor
        NodeKind::PreviousFrame        => Some(BUILTIN_PASS),

        NodeKind::ShaderPass | NodeKind::ColorCorrect
        | NodeKind::Blur | NodeKind::Keyer => Some(BUILTIN_PASS),

        // Feedback: iChannel0=live, iChannel1=previous frame
        NodeKind::Feedback             => Some(BUILTIN_FEEDBACK),

        NodeKind::Crossfade | NodeKind::KeyMix
        | NodeKind::ShaderMix2         => Some(BUILTIN_XFADE),
        NodeKind::ShaderMix3 | NodeKind::ShaderMix4
        | NodeKind::MatrixMix4         => Some(BUILTIN_XFADE),
        NodeKind::Add                  => Some(BUILTIN_ADD),
        NodeKind::Multiply             => Some(BUILTIN_MUL),

        NodeKind::Window | NodeKind::TextureOut | NodeKind::PixelsOut
        | NodeKind::Syphon | NodeKind::Spout | NodeKind::Recorder
        | NodeKind::Ndi   | NodeKind::Rtsp    => None,
    }
}

fn is_output(kind: &NodeKind) -> bool { builtin_frag(kind).is_none() }

/// Returns true if this node kind uses ping-pong (reads its own previous output).
fn is_pingpong(kind: &NodeKind) -> bool {
    matches!(kind, NodeKind::Feedback | NodeKind::PreviousFrame)
}

fn port_to_channel(name: &str) -> Option<usize> {
    match name {
        "in" | "in0" | "a" | "src"  => Some(0),
        "in1" | "b"  | "src1"       => Some(1),
        "in2" | "c"  | "src2"       => Some(2),
        "in3" | "d"  | "src3"       => Some(3),
        _ => None,
    }
}

// ── Texture resolution ────────────────────────────────────────────────────

/// Unified render target reference — either a plain target or a ping-pong write target.
enum TargetRef<'a> {
    Plain(&'a RenderTarget),
    PingPong(&'a PingPongTarget),
}

impl<'a> TargetRef<'a> {
    fn render_view(&self) -> &wgpu::TextureView {
        match self {
            Self::Plain(t)    => &t.render_view,
            Self::PingPong(p) => &p.write_target().render_view,
        }
    }
}

// ── Free functions ────────────────────────────────────────────────────────

/// Resolve iChannel0..3 views for a node.
/// For Feedback/PreviousFrame nodes, iChannel1 (or iChannel0 for PreviousFrame)
/// is overridden with the ping-pong read texture.
fn resolve_inputs(
    graph:      &Graph,
    plan:       &Plan,
    node_id:    NodeId,
    kind:       &NodeKind,
    targets:    &HashMap<NodeId, RenderTarget>,
    pingpongs:  &HashMap<NodeId, PingPongTarget>,
    blank:      &wgpu::Texture,
) -> [wgpu::TextureView; 4] {
    let bv = || blank.create_view(&wgpu::TextureViewDescriptor::default());
    let mut views: [Option<wgpu::TextureView>; 4] = [None, None, None, None];

    // Wire graph edges first
    for edge in plan.edges.iter().filter(|e| e.to.node == node_id) {
        if let Some(ch) = graph.node(node_id).and_then(|n| {
            n.ports.iter()
                .find(|p| p.id == edge.to.port)
                .and_then(|p| port_to_channel(&p.name))
        }) {
            // Get texture from upstream — check both plain and ping-pong targets
            if let Some(t) = targets.get(&edge.from.node) {
                views[ch] = Some(t.texture.create_view(&wgpu::TextureViewDescriptor::default()));
            } else if let Some(pp) = pingpongs.get(&edge.from.node) {
                // Upstream ping-pong node: expose its write target texture to downstream
                views[ch] = Some(pp.write_target().texture
                    .create_view(&wgpu::TextureViewDescriptor::default()));
            }
        }
    }

    // For ping-pong nodes: inject the previous frame's texture
    // PreviousFrame: overrides iChannel0 with the previous frame texture
    // Feedback: injects previous frame as iChannel1 (iChannel0 = live input from graph)
    if let Some(pp) = pingpongs.get(&node_id) {
        match kind {
            NodeKind::PreviousFrame => {
                views[0] = Some(pp.read_texture_view());
            }
            NodeKind::Feedback => {
                // iChannel1 = previous frame; iChannel0 stays as the live input from graph
                views[1] = Some(pp.read_texture_view());
            }
            _ => {}
        }
    }

    [
        views[0].take().unwrap_or_else(bv),
        views[1].take().unwrap_or_else(bv),
        views[2].take().unwrap_or_else(bv),
        views[3].take().unwrap_or_else(bv),
    ]
}

fn build_bind_group(
    device:         &wgpu::Device,
    layout:         &wgpu::BindGroupLayout,
    views:          &[wgpu::TextureView; 4],
    sampler:        &wgpu::Sampler,
    frame_uniforms: &UniformManager,
    custom_buffer:  &CustomUniformBuffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label:  Some("scheng_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&views[0]) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&views[1]) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&views[2]) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&views[3]) },
            wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(sampler) },
            wgpu::BindGroupEntry { binding: 5, resource: frame_uniforms.buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 6, resource: custom_buffer.buffer.as_entire_binding() },
        ],
    })
}

// ── WgpuRuntime ───────────────────────────────────────────────────────────

pub struct WgpuRuntime {
    pub ctx:     WgpuContext,
    pipelines:   PipelineCache,
    /// Plain render targets for normal nodes
    targets:     HashMap<NodeId, RenderTarget>,
    /// Ping-pong targets for Feedback/PreviousFrame nodes
    pingpongs:   HashMap<NodeId, PingPongTarget>,
    uniforms:    UniformManager,
    custom_bufs: HashMap<NodeId, CustomUniformBuffer>,
    blank:       wgpu::Texture,
    sampler:     wgpu::Sampler,
}

impl WgpuRuntime {
    pub fn new(width: u32, height: u32) -> Result<Self, WgpuError> {
        Self::new_inner(WgpuContext::new()?, width, height)
    }

    /// Create a runtime using an existing instance + surface.
    ///
    /// The instance must be the same one used to create the surface.
    /// This ensures the adapter and device are surface-compatible, which
    /// is required for the preview window blit to work without panicking.
    pub fn new_with_surface(
        instance: wgpu::Instance,
        surface:  &wgpu::Surface,
        width:    u32,
        height:   u32,
    ) -> Result<Self, WgpuError> {
        Self::new_inner(WgpuContext::new_with_surface(instance, surface)?, width, height)
    }

    fn new_inner(ctx: WgpuContext, width: u32, height: u32) -> Result<Self, WgpuError> {
        let uniforms = UniformManager::new(&ctx.device);
        let blank    = create_blank_texture(&ctx.device, &ctx.queue);
        let sampler  = ctx.device.create_sampler(&wgpu::SamplerDescriptor {
            label:          Some("scheng_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter:     wgpu::FilterMode::Linear,
            min_filter:     wgpu::FilterMode::Linear,
            mipmap_filter:  wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        log::info!("scheng-runtime-wgpu ready {}×{} — {}", width, height, ctx.adapter_info.name);
        Ok(Self {
            ctx,
            pipelines:   PipelineCache::new(),
            targets:     HashMap::new(),
            pingpongs:   HashMap::new(),
            uniforms,
            custom_bufs: HashMap::new(),
            blank,
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
        self.uniforms.update(&self.ctx.queue, ctx);

        // ── Phase A: create/resize all render targets ─────────────────────
        for &node_id in &plan.nodes {
            let node = graph.node(node_id).ok_or_else(||
                WgpuError::Wgpu(format!("Unknown NodeId {node_id:?}")))?;
            if is_output(&node.kind) { continue; }
            let label = format!("{:?}_{node_id:?}", node.kind);

            if is_pingpong(&node.kind) {
                let pp = self.pingpongs.entry(node_id).or_insert_with(||
                    PingPongTarget::new(&self.ctx.device, &self.ctx.queue,
                        ctx.width, ctx.height, &label));
                pp.ensure_size(&self.ctx.device, &self.ctx.queue,
                    ctx.width, ctx.height, &label);
            } else {
                let t = self.targets.entry(node_id).or_insert_with(||
                    RenderTarget::new(&self.ctx.device, ctx.width, ctx.height, &label));
                t.ensure_size(&self.ctx.device, ctx.width, ctx.height, &label);
            }
        }

        // ── Phase B: encode render passes ────────────────────────────────
        let mut encoder = self.ctx.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("scheng_frame") }
        );

        let mut pending_outputs: Vec<(NodeId, NodeId)> = Vec::new();

        for &node_id in &plan.nodes {
            let node = graph.node(node_id).ok_or_else(||
                WgpuError::Wgpu(format!("Unknown NodeId {node_id:?}")))?;
            let kind = &node.kind;

            if is_output(kind) {
                let upstream_id = plan.edges.iter()
                    .find(|e| e.to.node == node_id)
                    .map(|e| e.from.node)
                    .ok_or(WgpuError::NoRenderTarget(node_id))?;
                pending_outputs.push((node_id, upstream_id));
                continue;
            }

            let config   = configs.get(&node_id).ok_or(WgpuError::MissingNodeConfig(node_id))?;
            let frag_src = config.frag_shader.as_deref()
                .or_else(|| builtin_frag(kind))
                .unwrap_or(BUILTIN_SOURCE);
            let label    = format!("{kind:?}_{node_id:?}");

            let views = resolve_inputs(
                graph, plan, node_id, kind,
                &self.targets, &self.pingpongs, &self.blank,
            );

            let (pipeline, bgl, custom_names) = {
                let p = self.pipelines.get_or_create(&self.ctx.device, frag_src, &label)?;
                (&p.pipeline as *const _, &p.bind_group_layout as *const _, p.custom_uniform_names.clone())
            };
            let (pipeline, bgl) = unsafe { (&*pipeline, &*bgl) };

            let custom_buf = self.custom_bufs.entry(node_id)
                .or_insert_with(|| CustomUniformBuffer::new(&self.ctx.device, &label));
            custom_buf.update(&self.ctx.queue, &custom_names, &config.uniforms);

            let bg = build_bind_group(
                &self.ctx.device, bgl, &views,
                &self.sampler, &self.uniforms, custom_buf,
            );

            // Get the render view — either plain target or ping-pong write target
            let render_view = if is_pingpong(kind) {
                self.pingpongs.get(&node_id)
                    .expect("ping-pong missing after Phase A")
                    .write_target().texture
                    .create_view(&wgpu::TextureViewDescriptor::default())
            } else {
                self.targets.get(&node_id)
                    .expect("target missing after Phase A")
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default())
            };

            // We need to borrow render_view independently from self — clone avoids
            // the borrow conflict with self.pipelines above.
            // wgpu::TextureView is a ref-counted handle, clone is O(1).

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(&label),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view:           &render_view,
                        resolve_target: None,
                        ops:            wgpu::Operations {
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
            log::trace!("Rendered {label}");
        }

        // Submit render work BEFORE presenting or swapping
        self.ctx.queue.submit(std::iter::once(encoder.finish()));

        // Swap ping-pong buffers after submit — next frame reads what we just wrote
        for pp in self.pingpongs.values_mut() {
            pp.swap();
        }

        // Present output nodes
        for (node_id, upstream_id) in pending_outputs {
            // Upstream could be a plain target or a ping-pong write target
            let target = self.targets.get(&upstream_id)
                .or_else(|| {
                    // For ping-pong upstreams, expose the write target
                    // (swap already happened, so write_idx now points to the just-rendered frame)
                    // Actually after swap, read_idx = just rendered. Use read side.
                    None // handled below
                });

            if let Some(t) = target {
                sink.present(node_id, t, ctx, &self.ctx.device, &self.ctx.queue);
            } else if let Some(pp) = self.pingpongs.get(&upstream_id) {
                // After swap: write_target() is the OLD write target (just-rendered frame).
                // It's now the "read" side — exactly what we want to present.
                sink.present(node_id, pp.write_target(), ctx, &self.ctx.device, &self.ctx.queue);
            } else {
                return Err(WgpuError::NoRenderTarget(node_id));
            }
        }

        Ok(())
    }

    pub fn readback_pixels(&self, node_id: NodeId) -> Result<Vec<u8>, WgpuError> {
        if let Some(t) = self.targets.get(&node_id) {
            return Ok(t.readback(&self.ctx.device, &self.ctx.queue));
        }
        Err(WgpuError::NoRenderTarget(node_id))
    }
}

// ── PixelReadbackSink ─────────────────────────────────────────────────────

pub struct PixelReadbackSink {
    pixels: HashMap<NodeId, Vec<u8>>,
}

impl PixelReadbackSink {
    pub fn new() -> Self { Self { pixels: HashMap::new() } }
    pub fn take_pixels(&mut self, node_id: NodeId) -> Option<Vec<u8>> { self.pixels.remove(&node_id) }
    pub fn pixels(&self, node_id: NodeId) -> Option<&[u8]> { self.pixels.get(&node_id).map(|v| v.as_slice()) }
}

impl OutputSink for PixelReadbackSink {
    fn present(&mut self, node_id: NodeId, target: &RenderTarget, _ctx: &FrameCtx,
               device: &wgpu::Device, queue: &wgpu::Queue) {
        self.pixels.insert(node_id, target.readback(device, queue));
    }
}
