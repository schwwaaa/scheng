//! `executor.rs` — the main runtime entry point.
//!
//! [`WgpuRuntime`] owns the wgpu context and all GPU resources.
//! The instrument calls [`WgpuRuntime::execute_frame`] once per frame.
//!
//! # Frame execution lifecycle
//!
//! ```text
//! execute_frame(plan, configs, ctx)
//!   1. uniform_manager.update(ctx)          — write uTime/uResolution/uFrame
//!   2. for each NodeId in plan order:
//!      a. resolve shader source (NodeConfig or builtin)
//!      b. get/create render target (resize if needed)
//!      c. get/create pipeline + bind group layout
//!      d. build bind group (input textures + sampler + uniforms)
//!      e. encode render pass → draw 3 vertices (fullscreen triangle)
//!   3. queue.submit(encoder)
//!   4. for each OutputSink node: call sink.present(render_target)
//! ```
//!
//! # OutputSink trait
//!
//! Instrument code implements [`OutputSink`] to consume the rendered frame.
//! Phase 1 ships a `PixelReadbackSink` for testing.
//! Phase 3 will add Syphon, Spout, FFmpeg, and NDI sinks.

use std::collections::HashMap;

use scheng_core::FrameCtx;
use scheng_graph::{NodeId, NodeKind, Plan};

// TODO: import builtin_shader_for from scheng_runtime when you can verify the API.
// For Phase 1 we bundle a local fallback.
// use scheng_runtime::runtime_contract::builtin_shader_for;

use crate::{
    context::WgpuContext,
    pipeline::PipelineCache,
    render_target::{create_blank_texture, RenderTarget},
    uniforms::UniformManager,
    WgpuError,
};

// ── OutputSink trait ──────────────────────────────────────────────────────

/// Implemented by the host to consume the rendered output of a graph node.
///
/// Called once per Output node per frame, after all rendering is complete.
/// The sink receives a reference to the node's [`RenderTarget`] and can:
/// - read the texture handle for Syphon/Spout sharing
/// - submit it to an FFmpeg/NDI encoder
/// - read back pixels for testing
///
/// Sinks must NOT issue GPU commands (no encoder, no render pass).
/// GPU work at this stage is undefined behaviour.
pub trait OutputSink {
    /// Called after all nodes have rendered for this frame.
    ///
    /// `node_id` — which Output node's result this is.
    /// `target` — the render target containing the final rendered texture.
    /// `ctx` — the frame context (resolution, time, frame number).
    fn present(
        &mut self,
        node_id: NodeId,
        target: &RenderTarget,
        ctx: &FrameCtx,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    );
}

// ── NodeConfig ────────────────────────────────────────────────────────────

/// Per-node configuration supplied by the instrument each frame.
///
/// The runtime reads this alongside [`FrameCtx`] to execute each node.
/// It does not store NodeConfig internally — the instrument owns it.
///
/// # Relationship to scheng-runtime NodeProps
///
/// TODO: Once the scheng-runtime NodeProps API is confirmed, this struct
/// should either wrap NodeProps or be replaced by it. For Phase 1 we
/// keep it self-contained to avoid depending on an unverified internal API.
pub struct NodeConfig {
    /// Fragment shader source (GLSL 330 core).
    ///
    /// `None` means "use the built-in shader for this node kind".
    /// Built-in shaders are defined in [`BUILTIN_SHADERS`].
    pub frag_shader: Option<String>,

    /// Crossfade position [0.0, 1.0] for `Crossfade` mixer nodes.
    /// 0.0 = full channel A, 1.0 = full channel B.
    pub mix: f32,

    /// Per-channel gain weights for `MatrixMix4` nodes.
    /// Default `[1.0, 0.0, 0.0, 0.0]` passes iChannel0 through.
    pub matrix_weights: [f32; 4],

    /// For Output nodes: output name. `None` = primary output.
    /// `"main"` is reserved; use `None` for the primary PixelsOut.
    pub output_name: Option<String>,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            frag_shader: None,
            mix: 0.5,
            matrix_weights: [1.0, 0.0, 0.0, 0.0],
            output_name: None,
        }
    }
}

// ── Built-in shaders ──────────────────────────────────────────────────────
//
// Phase 1: minimal set matching the most important node kinds.
// TODO: replace with scheng_runtime::runtime_contract::builtin_shader_for(kind)
//       once you can verify the return type and path.

/// Get the built-in fragment shader for a node kind.
///
/// Returns `None` for Output nodes (they don't render — they consume).
fn builtin_frag(kind: NodeKind) -> Option<&'static str> {
    match kind {
        NodeKind::ShaderSource | NodeKind::NoiseSource => Some(BUILTIN_FRAG_SOLID),
        NodeKind::ShaderPass | NodeKind::ColorCorrect => Some(BUILTIN_FRAG_PASSTHROUGH),
        NodeKind::Crossfade => Some(BUILTIN_FRAG_CROSSFADE),
        NodeKind::Add => Some(BUILTIN_FRAG_ADD),
        NodeKind::Multiply => Some(BUILTIN_FRAG_MULTIPLY),
        NodeKind::Feedback | NodeKind::PreviousFrame => Some(BUILTIN_FRAG_PASSTHROUGH),
        // Output and unrecognised kinds don't render
        _ => None,
    }
}

// A checkerboard gradient — visually confirms the shader is running.
const BUILTIN_FRAG_SOLID: &str = r#"
void main() {
    vec2 uv = v_uv;
    // Animated gradient with time — proves uTime is wired correctly.
    float r = uv.x + 0.5 * sin(uTime);
    float g = uv.y + 0.5 * cos(uTime * 0.7);
    fragColor = vec4(r, g, 0.2, 1.0);
}
"#;

// Passes iChannel0 through unchanged.
const BUILTIN_FRAG_PASSTHROUGH: &str = r#"
void main() {
    fragColor = texture(iChannel0, v_uv);
}
"#;

// T-bar crossfade between iChannel0 (A) and iChannel1 (B).
// u_tbar would normally drive mix; for the builtin we hardcode 0.5
// until custom uniforms land in Phase 1.2.
const BUILTIN_FRAG_CROSSFADE: &str = r#"
void main() {
    vec4 a = texture(iChannel0, v_uv);
    vec4 b = texture(iChannel1, v_uv);
    fragColor = mix(a, b, 0.5); // TODO: replace 0.5 with u_tbar in Phase 1.2
}
"#;

const BUILTIN_FRAG_ADD: &str = r#"
void main() {
    fragColor = clamp(texture(iChannel0, v_uv) + texture(iChannel1, v_uv), 0.0, 1.0);
}
"#;

const BUILTIN_FRAG_MULTIPLY: &str = r#"
void main() {
    fragColor = texture(iChannel0, v_uv) * texture(iChannel1, v_uv);
}
"#;

// ── WgpuRuntime ───────────────────────────────────────────────────────────

/// The main wgpu runtime — the entry point for instrument code.
///
/// Create once at startup with [`WgpuRuntime::new`].
/// Call [`WgpuRuntime::execute_frame`] once per frame.
pub struct WgpuRuntime {
    /// GPU device and queue.
    pub ctx: WgpuContext,
    /// Render pipeline cache (one pipeline per unique fragment shader).
    pipelines: PipelineCache,
    /// Per-node offscreen render targets.
    render_targets: HashMap<NodeId, RenderTarget>,
    /// Frame uniform buffer (uTime, uResolution, uFrame).
    uniform_manager: UniformManager,
    /// 1×1 black texture — bound to unconnected iChannelN slots.
    blank_texture: wgpu::Texture,
    /// Shared linear sampler — used by all nodes.
    sampler: wgpu::Sampler,
    /// Default render resolution (used until the first execute_frame call).
    default_width: u32,
    default_height: u32,
}

impl WgpuRuntime {
    /// Initialise the wgpu runtime.
    ///
    /// `width` and `height` are the default render resolution. They can be
    /// overridden per-frame via [`FrameCtx`].
    ///
    /// Blocks the calling thread during GPU device initialisation.
    pub fn new(width: u32, height: u32) -> Result<Self, WgpuError> {
        let ctx = WgpuContext::new()?;

        let uniform_manager = UniformManager::new(&ctx.device);
        let blank_texture   = create_blank_texture(&ctx.device, &ctx.queue);
        let sampler         = create_sampler(&ctx.device);
        let pipelines       = PipelineCache::new();

        Ok(Self {
            ctx,
            pipelines,
            render_targets: HashMap::new(),
            uniform_manager,
            blank_texture,
            sampler,
            default_width: width,
            default_height: height,
        })
    }

    /// Execute one frame of the compiled plan.
    ///
    /// # Arguments
    /// - `plan` — the compiled graph plan (ordered node execution list)
    /// - `node_configs` — per-node configuration supplied by the instrument
    /// - `ctx` — frame context (resolution, time, frame counter)
    /// - `sink` — receives the final rendered frame for each Output node
    ///
    /// # Errors
    /// - [`WgpuError::MissingNodeConfig`] if a node has no config entry
    /// - [`WgpuError::GlslCompile`] if a shader fails to compile
    ///
    /// # Determinism
    /// Given identical `(plan, node_configs, ctx)`, the execution order and
    /// resource bindings are fully deterministic — matching scheng's contract.
    pub fn execute_frame(
        &mut self,
        plan: &Plan,
        node_configs: &HashMap<NodeId, NodeConfig>,
        ctx: &FrameCtx,
        sink: &mut dyn OutputSink,
    ) -> Result<(), WgpuError> {
        // 1. Update frame uniforms (uTime, uResolution, uFrame).
        self.uniform_manager.update(&self.ctx.queue, ctx);

        // 2. One command encoder for the entire frame.
        let mut encoder = self.ctx.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("scheng_frame") }
        );

        // Track which NodeId produced an output texture this frame.
        // Used to resolve iChannelN inputs for downstream nodes.
        // plan.edges() maps (src_node, dst_node, dst_port) — we build
        // a port→texture_view lookup for each node.
        //
        // TODO: verify Plan API — adjust if plan.edges() / plan.node_ids() differ.
        // Based on README: Plan is "lightweight ordered list of NodeIds and edges".

        // 3. Render each node in topological order.
        for &node_id in plan.node_ids() {
            let kind = plan.node_kind(node_id);

            // Output nodes don't render — they call the sink.
            if is_output_node(kind) {
                let target = self.render_targets.get(&node_id).ok_or_else(|| {
                    // The upstream node (connected to this output) should have rendered.
                    // If there's no render target, the graph is missing an upstream connection.
                    WgpuError::NoRenderTarget(node_id)
                })?;
                sink.present(node_id, target, ctx, &self.ctx.device, &self.ctx.queue);
                continue;
            }

            // Get node config.
            let config = node_configs.get(&node_id)
                .ok_or(WgpuError::MissingNodeConfig(node_id))?;

            // Resolve fragment shader source.
            let frag_src = config.frag_shader.as_deref()
                .or_else(|| builtin_frag(kind))
                .unwrap_or(BUILTIN_FRAG_SOLID);

            let label = format!("{:?}", node_id);

            // Ensure render target exists and matches current resolution.
            let target = self.render_targets.entry(node_id).or_insert_with(|| {
                RenderTarget::new(&self.ctx.device, ctx.width, ctx.height, &label)
            });
            target.ensure_size(&self.ctx.device, ctx.width, ctx.height, &label);

            // Get or create the render pipeline.
            let node_pipeline = self.pipelines.get_or_create(
                &self.ctx.device,
                frag_src,
                &label,
            )?;

            // Build the input texture views for this node's iChannel0..3 slots.
            let input_views = self.resolve_input_views(plan, node_id, ctx.width, ctx.height);

            // Build the bind group for this draw call.
            let bind_group = self.build_bind_group(
                &node_pipeline.bind_group_layout,
                &input_views,
            );

            // Encode the render pass.
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(&label),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target.render_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            // Clear to black before rendering.
                            // Shaders that don't cover every pixel will see black.
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                pass.set_pipeline(&node_pipeline.pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                // Draw fullscreen triangle: 3 vertices, 1 instance.
                pass.draw(0..3, 0..1);
            } // render pass drops here, encoding is recorded

            log::trace!("Rendered node {:?} ({})", node_id, kind_name(kind));
        }

        // 4. Submit all render commands.
        self.ctx.queue.submit(std::iter::once(encoder.finish()));

        Ok(())
    }

    /// Read back rendered pixels from an output node's render target.
    ///
    /// Returns tightly-packed RGBA bytes (4 bytes per pixel, row-major).
    /// The image origin is top-left (wgpu convention, Y-flipped vs OpenGL).
    ///
    /// **Intended for testing only.** Pixel readback is synchronous and
    /// expensive — do not call in the render loop.
    pub fn readback_pixels(&self, node_id: NodeId) -> Result<Vec<u8>, WgpuError> {
        let target = self.render_targets.get(&node_id)
            .ok_or(WgpuError::NoRenderTarget(node_id))?;
        Ok(target.readback(&self.ctx.device, &self.ctx.queue))
    }

    // ── Private helpers ───────────────────────────────────────────────────

    /// Collect the `wgpu::TextureView` for each iChannel slot of `node_id`.
    ///
    /// Returns an array [ch0, ch1, ch2, ch3].
    /// If a slot has no upstream connection, the blank 1×1 texture is used.
    ///
    /// TODO: this currently uses the Plan's edge information.
    /// Adjust `plan.input_texture_for(node, channel)` to match the actual API.
    fn resolve_input_views(
        &self,
        plan: &Plan,
        node_id: NodeId,
        _width: u32,
        _height: u32,
    ) -> [wgpu::TextureView; 4] {
        // Build blank views as the default.
        let blank_view = || self.blank_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut views: [Option<wgpu::TextureView>; 4] = [None, None, None, None];

        // Ask the plan which upstream node feeds each channel of this node.
        // TODO: verify plan.inputs_for(node_id) returns [(upstream_id, channel_index)].
        //       Adjust this block to match the actual scheng-graph Plan API.
        for (upstream_id, channel) in plan.inputs_for(node_id) {
            if channel < 4 {
                if let Some(target) = self.render_targets.get(&upstream_id) {
                    views[channel as usize] = Some(
                        target.texture.create_view(&wgpu::TextureViewDescriptor::default())
                    );
                }
            }
        }

        [
            views[0].take().unwrap_or_else(blank_view),
            views[1].take().unwrap_or_else(blank_view),
            views[2].take().unwrap_or_else(blank_view),
            views[3].take().unwrap_or_else(blank_view),
        ]
    }

    /// Build a bind group that wires textures, sampler, and uniforms.
    fn build_bind_group(
        &self,
        layout: &wgpu::BindGroupLayout,
        input_views: &[wgpu::TextureView; 4],
    ) -> wgpu::BindGroup {
        self.ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scheng_bind_group"),
            layout,
            entries: &[
                // iChannel0..3 — texture views
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&input_views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&input_views[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&input_views[2]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&input_views[3]),
                },
                // iSampler — shared linear filtering sampler
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                // FrameBlock — frame uniform buffer
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: self.uniform_manager.buffer.as_entire_binding(),
                },
            ],
        })
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn create_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("scheng_sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    })
}

fn is_output_node(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Window
            | NodeKind::PixelsOut
            | NodeKind::TextureOut
            | NodeKind::Syphon
            | NodeKind::Spout
            | NodeKind::Recorder
            | NodeKind::Ndi
            | NodeKind::Rtsp
    )
}

fn kind_name(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::ShaderSource => "ShaderSource",
        NodeKind::ShaderPass   => "ShaderPass",
        NodeKind::Crossfade    => "Crossfade",
        NodeKind::Add          => "Add",
        NodeKind::Multiply     => "Multiply",
        NodeKind::PixelsOut    => "PixelsOut",
        _                      => "other",
    }
}

// ── PixelReadbackSink — for testing ───────────────────────────────────────

/// An [`OutputSink`] that reads pixels back to CPU memory.
///
/// Use this in tests and CLI tools to verify rendering output.
///
/// ```rust,no_run
/// let mut sink = PixelReadbackSink::new();
/// runtime.execute_frame(&plan, &configs, &ctx, &mut sink)?;
/// let pixels = sink.take_pixels(output_node_id).unwrap();
/// assert_eq!(pixels.len(), 1280 * 720 * 4); // RGBA
/// ```
pub struct PixelReadbackSink {
    pixels: HashMap<NodeId, Vec<u8>>,
}

impl PixelReadbackSink {
    pub fn new() -> Self {
        Self { pixels: HashMap::new() }
    }

    /// Take the pixel data for a node after `execute_frame` has run.
    /// Returns `None` if the node didn't render (not in the plan, or no output).
    pub fn take_pixels(&mut self, node_id: NodeId) -> Option<Vec<u8>> {
        self.pixels.remove(&node_id)
    }

    /// Borrow the pixel data without consuming it.
    pub fn pixels(&self, node_id: NodeId) -> Option<&[u8]> {
        self.pixels.get(&node_id).map(|v| v.as_slice())
    }
}

impl OutputSink for PixelReadbackSink {
    fn present(
        &mut self,
        node_id: NodeId,
        target: &RenderTarget,
        _ctx: &FrameCtx,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let pixels = target.readback(device, queue);
        self.pixels.insert(node_id, pixels);
    }
}
