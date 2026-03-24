//! `pipeline.rs` — render pipeline and bind group layout management.
//!
//! [`PipelineCache`] creates and caches one [`wgpu::RenderPipeline`] per unique
//! fragment shader (keyed by source hash). Pipeline creation is expensive;
//! caching ensures it only happens when the shader source changes.
//!
//! # Bind group layout (fixed — matches compat header)
//!
//! All scheng nodes share one bind group layout at group 0:
//!
//! | binding | type              | stage    |
//! |--------:|-------------------|----------|
//! |       0 | Texture2D         | FRAGMENT |
//! |       1 | Texture2D         | FRAGMENT |
//! |       2 | Texture2D         | FRAGMENT |
//! |       3 | Texture2D         | FRAGMENT |
//! |       4 | Sampler(Filter)   | FRAGMENT |
//! |       5 | UniformBuffer     | FRAGMENT |
//!
//! This matches `compat.rs`'s injected header exactly.
//!
//! # Pipeline layout
//!
//! One bind group (group 0) containing all resources.
//! No push constants, no vertex buffers (fullscreen triangle uses `vertex_index`).

use std::collections::HashMap;

use crate::{
    render_target::RENDER_TARGET_FORMAT,
    shader::{ShaderCache, VERTEX_SHADER_WGSL},
    WgpuError,
};

/// A compiled render pipeline and its associated bind group layout.
pub struct NodePipeline {
    pub pipeline:             wgpu::RenderPipeline,
    pub bind_group_layout:    wgpu::BindGroupLayout,
    pub custom_uniform_names: Vec<String>,
}

/// Cache of compiled render pipelines, keyed by fragment shader source hash.
pub struct PipelineCache {
    /// Compiled pipelines, keyed by fragment shader source hash.
    pipelines: HashMap<(u64, u32), NodePipeline>,
    /// Shader module cache (vertex + fragment modules).
    shaders: ShaderCache,
}

impl PipelineCache {
    pub fn new() -> Self {
        Self { pipelines: HashMap::new(), shaders: ShaderCache::new() }
    }

    /// Get or create the render pipeline for a given fragment shader source.
    ///
    /// `frag_src` is the **original GLSL 330** source — compat preprocessing
    /// happens inside [`ShaderCache::fragment_module`].
    ///
    /// The resulting pipeline is cached so subsequent calls with the same
    /// source string are O(1).
    pub fn get_or_create<'a>(
        &'a mut self,
        device: &wgpu::Device,
        frag_src: &str,
        node_label: &str,
        sample_count: u32,
    ) -> Result<&'a NodePipeline, WgpuError> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut h = DefaultHasher::new();
        frag_src.hash(&mut h);
        let hash = h.finish();
        let key = (hash, sample_count);

        if !self.pipelines.contains_key(&key) {
            let pipeline = build_pipeline(
                device,
                &mut self.shaders,
                frag_src,
                node_label,
                sample_count,
            )?;
            self.pipelines.insert(key, pipeline);
        }

        Ok(self.pipelines.get(&key).unwrap())
    }

    /// Clear the pipeline cache (e.g. on hot-reload).
    pub fn clear(&mut self) {
        self.pipelines.clear();
        self.shaders.clear();
    }
}

// ── Pipeline construction ─────────────────────────────────────────────────

/// Build a single render pipeline for one fragment shader.
fn build_pipeline(
    device: &wgpu::Device,
    shaders: &mut ShaderCache,
    frag_src: &str,
    node_label: &str,
    sample_count: u32,
) -> Result<NodePipeline, WgpuError> {
    // Compile/get shader modules.
    let vert_module = {
        // Safety: we borrow shaders mutably for vert, then drop before frag borrow.
        // The vertex shader is always the built-in WGSL — we cache it separately
        // to avoid lifetime issues. Get it by value (wgpu::ShaderModule isn't Clone,
        // so we re-create it if not cached — it's fast for WGSL).
        // TODO: ShaderCache should store Arc<wgpu::ShaderModule> to allow sharing.
        // For Phase 1, create the vertex module inline (it's tiny and cached by wgpu).
        device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scheng_vert"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(VERTEX_SHADER_WGSL)),
        })
    };

    let (frag_module, custom_uniform_names) = shaders.fragment_module_with_names(device, frag_src, node_label)?;

    // Build the shared bind group layout.
    let bind_group_layout = build_bind_group_layout(device, node_label);

    // Build the pipeline layout.
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&format!("{}_layout", node_label)),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    // Build the render pipeline.
    //
    // NOTE on `entry_point`: WGSL uses "vs_main"; GLSL compiled via naga
    // exposes the original entry point name, which for GLSL void main() is "main".
    //
    // NOTE on wgpu API version: in wgpu ≥ 0.20 / 22, VertexState and
    // FragmentState have a `compilation_options` field. If your version
    // doesn't have it, remove those lines.
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(&format!("{}_pipeline", node_label)),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &vert_module,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            // No vertex buffers — fullscreen triangle uses @builtin(vertex_index)
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: frag_module,
            // GLSL `void main()` → naga entry point name is "main"
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: RENDER_TARGET_FORMAT,
                // No blending — each node outputs a complete opaque frame.
                // Blending semantics (for mixer nodes like Crossfade/Add)
                // are handled in the fragment shader itself.
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None, // fullscreen triangle — cull mode doesn't matter
            ..Default::default()
        },
        depth_stencil: None, // video pipeline never uses depth
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    });

    Ok(NodePipeline { pipeline, bind_group_layout, custom_uniform_names })
}

// ── Bind group layout ─────────────────────────────────────────────────────

/// Build the bind group layout that matches our compat header's binding slots.
///
/// All nodes share this layout — the specific textures bound to each slot
/// change per-node per-frame, but the layout itself is constant.
pub fn build_bind_group_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(&format!("{}_bgl", label)),
        entries: &[
            // binding 0 — iChannel0_tex
            tex_entry(0),
            // binding 1 — iChannel1_tex
            tex_entry(1),
            // binding 2 — iChannel2_tex
            tex_entry(2),
            // binding 3 — iChannel3_tex
            tex_entry(3),
            // binding 4 — iSampler (shared linear filtering sampler)
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            // binding 5 — FrameBlock uniform buffer
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // binding 6 — CustomBlock (per-node u_* uniforms)
            wgpu::BindGroupLayoutEntry {
                binding: 6,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

/// Convenience helper: a texture2D binding entry.
fn tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}
