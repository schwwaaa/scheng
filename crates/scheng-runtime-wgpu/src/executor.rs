//! `executor.rs` — WgpuRuntime: execute_frame, OutputSink, NodeConfig.
//!
//! Plan API used here (verified from scheng-graph source):
//!   plan.nodes: Vec<NodeId>   — topological order
//!   plan.edges: Vec<Edge>     — Edge { from: Endpoint, to: Endpoint }
//!   Endpoint { node: NodeId, port: PortId }
//!   graph.node(id) -> Option<&Node>
//!   Node { kind: NodeKind, ports: Vec<Port> }
//!   Port { id: PortId, name: String }

use std::collections::HashMap;

use scheng_graph::{Graph, NodeId, NodeKind, Plan};

use crate::{
    context::WgpuContext,
    pipeline::PipelineCache,
    render_target::{create_blank_texture, RenderTarget},
    uniforms::UniformManager,
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

/// Per-node configuration supplied by the instrument.
pub struct NodeConfig {
    /// GLSL 330 fragment shader. `None` → built-in for this NodeKind.
    pub frag_shader: Option<String>,
    /// Output name for PixelsOut nodes. `None` = primary output.
    pub output_name: Option<String>,
}

impl Default for NodeConfig {
    fn default() -> Self { Self { frag_shader: None, output_name: None } }
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

fn builtin_frag(kind: &NodeKind) -> Option<&'static str> {
    match kind {
        NodeKind::ShaderSource | NodeKind::NoiseSource
        | NodeKind::PreviousFrame | NodeKind::TextureInputPass
        | NodeKind::VideoDecodeSource  => Some(BUILTIN_SOURCE),

        NodeKind::ShaderPass | NodeKind::ColorCorrect
        | NodeKind::Blur | NodeKind::Keyer
        | NodeKind::Feedback           => Some(BUILTIN_PASS),

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

fn port_to_channel(name: &str) -> Option<usize> {
    match name {
        "in" | "in0" | "a" | "src"  => Some(0),
        "in1" | "b"  | "src1"       => Some(1),
        "in2" | "c"  | "src2"       => Some(2),
        "in3" | "d"  | "src3"       => Some(3),
        _ => None,
    }
}

// ── Free functions (avoid self borrow conflicts) ──────────────────────────

fn resolve_inputs(
    graph:   &Graph,
    plan:    &Plan,
    node_id: NodeId,
    targets: &HashMap<NodeId, RenderTarget>,
    blank:   &wgpu::Texture,
) -> [wgpu::TextureView; 4] {
    let bv = || blank.create_view(&wgpu::TextureViewDescriptor::default());
    let mut views: [Option<wgpu::TextureView>; 4] = [None, None, None, None];

    for edge in plan.edges.iter().filter(|e| e.to.node == node_id) {
        if let Some(ch) = graph.node(node_id).and_then(|n| {
            n.ports.iter()
                .find(|p| p.id == edge.to.port)
                .and_then(|p| port_to_channel(&p.name))
        }) {
            if let Some(t) = targets.get(&edge.from.node) {
                views[ch] = Some(t.texture.create_view(&wgpu::TextureViewDescriptor::default()));
            }
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
    device:   &wgpu::Device,
    layout:   &wgpu::BindGroupLayout,
    views:    &[wgpu::TextureView; 4],
    sampler:  &wgpu::Sampler,
    uniforms: &UniformManager,
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
            wgpu::BindGroupEntry { binding: 5, resource: uniforms.buffer.as_entire_binding() },
        ],
    })
}

// ── WgpuRuntime ───────────────────────────────────────────────────────────

/// Main wgpu runtime. Create once; call `execute_frame` each frame.
pub struct WgpuRuntime {
    /// GPU device and queue (exposed for OutputSink implementations).
    pub ctx:     WgpuContext,
    pipelines:   PipelineCache,
    targets:     HashMap<NodeId, RenderTarget>,
    uniforms:    UniformManager,
    blank:       wgpu::Texture,
    sampler:     wgpu::Sampler,
}

impl WgpuRuntime {
    /// Create a new runtime. Blocks until the GPU device is ready.
    pub fn new(width: u32, height: u32) -> Result<Self, WgpuError> {
        let ctx      = WgpuContext::new()?;
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
        Ok(Self { ctx, pipelines: PipelineCache::new(), targets: HashMap::new(), uniforms, blank, sampler })
    }

    /// Execute one frame of the compiled plan.
    pub fn execute_frame(
        &mut self,
        graph:   &Graph,
        plan:    &Plan,
        configs: &HashMap<NodeId, NodeConfig>,
        ctx:     &FrameCtx,
        sink:    &mut dyn OutputSink,
    ) -> Result<(), WgpuError> {
        self.uniforms.update(&self.ctx.queue, ctx);

        // Phase A — create/resize all render targets (isolated mut borrow).
        for &node_id in &plan.nodes {
            let node = graph.node(node_id).ok_or_else(||
                WgpuError::Wgpu(format!("Unknown NodeId {node_id:?}"))
            )?;
            if is_output(&node.kind) { continue; }
            let label = format!("{:?}_{node_id:?}", node.kind);
            let t = self.targets.entry(node_id).or_insert_with(||
                RenderTarget::new(&self.ctx.device, ctx.width, ctx.height, &label)
            );
            t.ensure_size(&self.ctx.device, ctx.width, ctx.height, &label);
        }
        // mut borrow of self.targets ends here.

        // Phase B — encode render passes.
        let mut encoder = self.ctx.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("scheng_frame") }
        );

        // Collect output (sink) nodes to call AFTER submit.
        // sink.present() does a texture readback — the GPU must have finished
        // the render pass before the copy makes sense. Calling it before
        // queue.submit() reads an empty/stale texture (all zeros).
        let mut pending_outputs: Vec<(NodeId, NodeId)> = Vec::new();

        for &node_id in &plan.nodes {
            let node = graph.node(node_id).ok_or_else(||
                WgpuError::Wgpu(format!("Unknown NodeId {node_id:?}"))
            )?;
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

            // Resolve inputs BEFORE the mut pipeline borrow.
            let views = resolve_inputs(graph, plan, node_id, &self.targets, &self.blank);

            // Mut borrow of self.pipelines — isolated to this block.
            let (pipeline, bgl) = {
                let p = self.pipelines.get_or_create(&self.ctx.device, frag_src, &label)?;
                (&p.pipeline as *const wgpu::RenderPipeline,
                 &p.bind_group_layout as *const wgpu::BindGroupLayout)
            };
            // SAFETY: NodePipeline lives inside a HashMap entry that is never
            // removed during a frame. The raw pointer re-borrows as shared
            // references, which is sound because we do not mutate pipelines
            // after this point in the loop iteration.
            let (pipeline, bgl) = unsafe { (&*pipeline, &*bgl) };

            let bg = build_bind_group(&self.ctx.device, bgl, &views, &self.sampler, &self.uniforms);

            let render_view = &self.targets.get(&node_id)
                .expect("target missing after Phase A")
                .render_view;

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(&label),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view:           render_view,
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

        // Submit all render commands FIRST.
        self.ctx.queue.submit(std::iter::once(encoder.finish()));

        // Now call sink.present — GPU work is submitted, readback will see real pixels.
        for (node_id, upstream_id) in pending_outputs {
            let target = self.targets.get(&upstream_id)
                .ok_or(WgpuError::NoRenderTarget(node_id))?;
            sink.present(node_id, target, ctx, &self.ctx.device, &self.ctx.queue);
        }

        Ok(())
    }

    /// Read back RGBA pixels from a node's render target. Testing only.
    pub fn readback_pixels(&self, node_id: NodeId) -> Result<Vec<u8>, WgpuError> {
        let t = self.targets.get(&node_id).ok_or(WgpuError::NoRenderTarget(node_id))?;
        Ok(t.readback(&self.ctx.device, &self.ctx.queue))
    }
}

// ── PixelReadbackSink ─────────────────────────────────────────────────────

/// Reads pixels to CPU memory after each frame. For testing only.
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
    fn present(&mut self, node_id: NodeId, target: &RenderTarget, _ctx: &FrameCtx,
               device: &wgpu::Device, queue: &wgpu::Queue) {
        self.pixels.insert(node_id, target.readback(device, queue));
    }
}
