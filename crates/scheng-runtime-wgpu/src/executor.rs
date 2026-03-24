//! `executor.rs` — the main runtime entry point.
//!
//! # Plan API (verified from scheng-graph source)
//!
//! ```text
//! Plan     { pub nodes: Vec<NodeId>, pub edges: Vec<Edge> }
//! Edge     { pub from: Endpoint, pub to: Endpoint }
//! Endpoint { pub node: NodeId, pub port: PortId, pub dir: PortDir }
//! Node     { pub id: NodeId, pub kind: NodeKind, pub ports: Vec<Port> }
//! ```
//!
//! # Borrow strategy
//!
//! `self.pipelines.get_or_create()` returns a `&NodePipeline` that keeps
//! `self.pipelines` mutably borrowed for the reference's lifetime.
//! Calling `&self` methods while that reference is live causes E0502.
//!
//! Fix: `resolve_inputs` and `build_bind_group` are free functions that take
//! individual struct fields. Rust tracks field borrows independently, so
//! `&mut self.pipelines` and `&self.render_targets` can coexist.

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

/// Implemented by the host to consume the rendered output of a graph node.
pub trait OutputSink {
    /// Called once per Output node per frame after all GPU work is submitted.
    ///
    /// `target` holds the rendered texture. The sink can read pixels back,
    /// share the texture via Syphon/Spout, or feed an encoder.
    /// Do not issue wgpu commands from inside this method.
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
// Defined in scheng-param-store to avoid dependency cycle.
// Re-exported here for backward compatibility.
pub use scheng_param_store::NodeConfig;

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

const BUILTIN_FRAG_CROSSFADE: &str = r#"
void main() {
    vec4 a = texture(iChannel0, v_uv);
    vec4 b = texture(iChannel1, v_uv);
    fragColor = mix(a, b, 0.5);
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

/// `&NodeKind` — NodeKind does not implement Copy.
fn builtin_frag(kind: &NodeKind) -> Option<&'static str> {
    match kind {
        NodeKind::ShaderSource
        | NodeKind::NoiseSource
        | NodeKind::PreviousFrame
        | NodeKind::TextureInputPass
        | NodeKind::VideoDecodeSource => Some(BUILTIN_FRAG_SOURCE),

        NodeKind::ShaderPass
        | NodeKind::ColorCorrect
        | NodeKind::Blur
        | NodeKind::Keyer
        | NodeKind::Feedback => Some(BUILTIN_FRAG_PASSTHROUGH),

        NodeKind::Crossfade
        | NodeKind::KeyMix
        | NodeKind::ShaderMix2 => Some(BUILTIN_FRAG_CROSSFADE),

        // Phase 1.2: proper multi-input support; crossfade as placeholder
        NodeKind::ShaderMix3
        | NodeKind::ShaderMix4
        | NodeKind::MatrixMix4 => Some(BUILTIN_FRAG_CROSSFADE),

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

/// Port name → iChannel index (matches runtime_contract::input_channel_for).
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

/// Resolve iChannel0..3 texture views by walking plan.edges.
/// Takes individual fields so it doesn't conflict with &mut self.pipelines.
fn resolve_inputs(
    graph:          &Graph,
    plan:           &Plan,
    node_id:        NodeId,
    render_targets: &HashMap<NodeId, RenderTarget>,
    blank_texture:  &wgpu::Texture,
    config:         &NodeConfig,
) -> [wgpu::TextureView; 4] {
    let blank = || blank_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut views: [Option<wgpu::TextureView>; 4] = [None, None, None, None];

    // External overrides (webcam, NDI receive, etc.) take priority over graph edges
    for (ch, slot) in config.input_textures.iter().enumerate() {
        if let Some(tex) = slot {
            views[ch] = Some(tex.create_view(&wgpu::TextureViewDescriptor::default()));
        }
    }

    // Graph edges fill remaining slots
    for edge in plan.edges.iter().filter(|e| e.to.node == node_id) {
        let channel = graph.node(node_id).and_then(|n| {
            n.ports.iter()
                .find(|p| p.id == edge.to.port)
                .and_then(|p| port_name_to_channel(&p.name))
        });
        if let Some(ch) = channel {
            if views[ch].is_none() {
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

/// Build the bind group for one draw call.
/// Takes individual fields so it doesn't conflict with &mut self.pipelines.
fn build_bind_group(
    device:          &wgpu::Device,
    layout:          &wgpu::BindGroupLayout,
    views:           &[wgpu::TextureView; 4],
    sampler:         &wgpu::Sampler,
    uniform_manager: &UniformManager,
    custom_buffer:   &crate::uniforms::CustomUniformBuffer,
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
            wgpu::BindGroupEntry { binding: 5, resource: uniform_manager.buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 6, resource: custom_buffer.buffer.as_entire_binding() },
        ],
    })
}

// ── WgpuRuntime ───────────────────────────────────────────────────────────

/// The main wgpu runtime. Create once; call `execute_frame` each frame.
pub struct WgpuRuntime {
    /// The wgpu device and queue. Exposed for use by OutputSink implementations
    /// (e.g. Syphon/Spout sinks that need the device to create shared textures).
    pub ctx:         WgpuContext,
    pipelines:       PipelineCache,
    render_targets:  HashMap<NodeId, RenderTarget>,
    uniform_manager:        UniformManager,
    custom_uniform_buffers: HashMap<NodeId, crate::uniforms::CustomUniformBuffer>,
    blank_texture:   wgpu::Texture,
    sampler:         wgpu::Sampler,
}

impl WgpuRuntime {
    /// Initialise. Blocks until the GPU device is ready.
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
            uniform_manager,
            custom_uniform_buffers: HashMap::new(),
            blank_texture,
            sampler,
        })
    }

    /// Execute one frame.
    pub fn execute_frame(
        &mut self,
        graph:   &Graph,
        plan:    &Plan,
        configs: &HashMap<NodeId, NodeConfig>,
        ctx:     &FrameCtx,
        sink:    &mut dyn OutputSink,
    ) -> Result<(), WgpuError> {
        // Upload uTime / uResolution / uFrame.
        self.uniform_manager.update(&self.ctx.queue, ctx);

        // ── Phase A: create/resize all render targets ─────────────────────
        // Mutable borrow of self.render_targets is isolated to this block.
        for &node_id in &plan.nodes {
            let node = graph.node(node_id).ok_or_else(||
                WgpuError::Wgpu(format!("Plan references unknown NodeId {node_id:?}"))
            )?;
            if is_output_kind(&node.kind) { continue; }
            let label = format!("{:?}_{node_id:?}", node.kind);
            let target = self.render_targets
                .entry(node_id)
                .or_insert_with(|| RenderTarget::new(&self.ctx.device, ctx.width, ctx.height, &label));
            target.ensure_size_msaa(&self.ctx.device, ctx.width, ctx.height, ctx.sample_count, &label);
        }
        // render_targets mutable borrow ends here.

        // ── Phase B: encode render passes ────────────────────────────────
        let mut encoder = self.ctx.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("scheng_frame") }
        );

        for &node_id in &plan.nodes {
            let node = graph.node(node_id).ok_or_else(||
                WgpuError::Wgpu(format!("Plan references unknown NodeId {node_id:?}"))
            )?;
            let kind = &node.kind;

            // Output nodes are presented after queue.submit() below
            if is_output_kind(kind) { continue; }

            let config = configs.get(&node_id)
                .ok_or(WgpuError::MissingNodeConfig(node_id))?;

            let frag_src = config.frag_shader.as_deref()
                .or_else(|| builtin_frag(kind))
                .unwrap_or(BUILTIN_FRAG_SOURCE);

            let label = format!("{kind:?}_{node_id:?}");

            // Resolve inputs BEFORE the mutable pipeline borrow begins.
            // Free function — takes &self.render_targets and &self.blank_texture
            // directly, so it doesn't block the subsequent &mut self.pipelines.
            let input_views = resolve_inputs(
                graph, plan, node_id,
                &self.render_targets,
                &self.blank_texture,
                config,
            );

            // Mutable borrow of self.pipelines starts here.
            let node_pipeline = self.pipelines.get_or_create(
                &self.ctx.device, frag_src, &label, ctx.sample_count
            )?;

            // Get or create the per-node custom uniform buffer and upload values.
            let custom_buf = self.custom_uniform_buffers
                .entry(node_id)
                .or_insert_with(|| crate::uniforms::CustomUniformBuffer::new(
                    &self.ctx.device, &label
                ));
            custom_buf.update(
                &self.ctx.queue,
                &node_pipeline.custom_uniform_names,
                &config.uniforms,
            );

            // build_bind_group is a free function — takes &self.ctx.device,
            // &self.sampler, &self.uniform_manager directly, which are all
            // distinct fields from self.pipelines.
            let bind_group = build_bind_group(
                &self.ctx.device,
                &node_pipeline.bind_group_layout,
                &input_views,
                &self.sampler,
                &self.uniform_manager,
                custom_buf,
            );

            // Borrow the render view. render_targets is a distinct field from
            // pipelines so this immutable borrow is compatible.
            // Borrow attachment and resolve views from the render target.
            // When MSAA is active: render to msaa_view, resolve to render_view.
            // When MSAA is off:   render directly to render_view.
            let (attachment, resolve) = {
                let rt = self.render_targets
                    .get(&node_id)
                    .expect("render target missing after Phase A");
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
            };

            {
                // SAFETY: render_targets field is not borrowed mutably below.
                let attachment = unsafe { &*attachment };
                let resolve_target = resolve.map(|p| unsafe { &*p });

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
                pass.draw(0..3, 0..1);
            }

            log::trace!("Rendered {label}");
        }

        self.ctx.queue.submit(std::iter::once(encoder.finish()));

        // ── Phase C: present output nodes (after GPU work is submitted) ──────
        for &node_id in &plan.nodes {
            let node = graph.node(node_id).ok_or_else(||
                WgpuError::Wgpu(format!("Plan references unknown NodeId {node_id:?}"))
            )?;
            if !is_output_kind(&node.kind) { continue; }
            let upstream_id = plan.edges.iter()
                .find(|e| e.to.node == node_id)
                .map(|e| e.from.node);
            let target = upstream_id
                .and_then(|id| self.render_targets.get(&id))
                .ok_or(WgpuError::NoRenderTarget(node_id))?;
            sink.present(node_id, target, ctx, &self.ctx.device, &self.ctx.queue);
        }

        Ok(())
    }

    /// Read back RGBA pixels from a node's render target. Testing only.
    pub fn readback_pixels(&self, node_id: NodeId) -> Result<Vec<u8>, WgpuError> {
        let target = self.render_targets.get(&node_id)
            .ok_or(WgpuError::NoRenderTarget(node_id))?;
        Ok(target.readback(&self.ctx.device, &self.ctx.queue))
    }
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
