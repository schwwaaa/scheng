//! `pipeline.rs` — render pipeline and bind group layout management.
//!
//! Two pipeline families:
//!
//! 1. **Fullscreen pipeline** (default): 3-vertex triangle, no vertex buffer.
//!    Used by every ShaderSource, ShaderPass, Crossfade, etc.
//!
//! 2. **Geometry pipeline**: explicit vertex buffer of `[f32; 2]` NDC positions.
//!    Used by MeshSource / MSH3 nodes with `PipelineTopology::LineList`,
//!    `TriangleList`, or `PointList`.
//!
//! Both families share **one bind group layout** (bindings 0–7), so there is
//! only one layout object and all nodes can use the same descriptor set
//! structure. Binding 7 (MvpBlock) is always present; fullscreen nodes just
//! upload an identity matrix.
//!
//! # Bind group layout (fixed — matches compat header)
//!
//! | binding | type            | stage    |
//! |--------:|-----------------|----------|
//! |       0 | Texture2D       | FRAGMENT |
//! |       1 | Texture2D       | FRAGMENT |
//! |       2 | Texture2D       | FRAGMENT |
//! |       3 | Texture2D       | FRAGMENT |
//! |       4 | Sampler(Filter) | FRAGMENT |
//! |       5 | UniformBuffer   | FRAGMENT | FrameBlock
//! |       6 | UniformBuffer   | FRAGMENT | CustomBlock (u_* params)
//! |       7 | UniformBuffer   | VERTEX   | MvpBlock (geometry matrix)
//!
//! Binding 7 is VERTEX-stage only (fragment shaders never need the MVP).

use std::collections::HashMap;

use scheng_param_store::node_config::PipelineTopology;

use crate::{
    render_target::RENDER_TARGET_FORMAT,
    shader::{ShaderCache, VERTEX_SHADER_WGSL, VERTEX_SHADER_GEOMETRY_WGSL},
    WgpuError,
};

/// A compiled render pipeline and its associated bind group layout.
pub struct NodePipeline {
    pub pipeline:             wgpu::RenderPipeline,
    pub bind_group_layout:    wgpu::BindGroupLayout,
    pub custom_uniform_names: Vec<String>,
    pub topology:             PipelineTopology,
}

/// Cache key: (frag_hash, msaa_count, topology)
type PipelineKey = (u64, u32, PipelineTopology);

/// Cache of compiled render pipelines.
pub struct PipelineCache {
    pipelines: HashMap<PipelineKey, NodePipeline>,
    shaders:   ShaderCache,
}

impl PipelineCache {
    pub fn new() -> Self {
        Self { pipelines: HashMap::new(), shaders: ShaderCache::new() }
    }

    /// Get or create the render pipeline for a given fragment shader + topology.
    ///
    /// `frag_src` is the **original GLSL 330** source.
    /// `topology` selects fullscreen vs geometry pipeline family.
    pub fn get_or_create<'a>(
        &'a mut self,
        device:       &wgpu::Device,
        frag_src:     &str,
        node_label:   &str,
        sample_count: u32,
        topology:     PipelineTopology,
    ) -> Result<&'a NodePipeline, WgpuError> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut h = DefaultHasher::new();
        frag_src.hash(&mut h);
        let hash = h.finish();
        let key  = (hash, sample_count, topology);

        if !self.pipelines.contains_key(&key) {
            let pipeline = build_pipeline(
                device, &mut self.shaders,
                frag_src, node_label, sample_count, topology,
            )?;
            self.pipelines.insert(key, pipeline);
        }

        Ok(self.pipelines.get(&key).unwrap())
    }

    pub fn clear(&mut self) {
        self.pipelines.clear();
        self.shaders.clear();
    }
}

// ── Pipeline construction ─────────────────────────────────────────────────

fn build_pipeline(
    device:       &wgpu::Device,
    shaders:      &mut ShaderCache,
    frag_src:     &str,
    node_label:   &str,
    sample_count: u32,
    topology:     PipelineTopology,
) -> Result<NodePipeline, WgpuError> {

    // ── Vertex shader ────────────────────────────────────────────────────
    // Fullscreen: built-in WGSL (no vertex buffer, uses vertex_index builtin)
    // Geometry:   separate WGSL that reads @location(0) vec2<f32> positions
    let vert_src = if topology.is_geometry() {
        VERTEX_SHADER_GEOMETRY_WGSL
    } else {
        VERTEX_SHADER_WGSL
    };

    let vert_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label:  Some(&format!("{node_label}_vert")),
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(vert_src)),
    });

    // ── Fragment shader ──────────────────────────────────────────────────
    let (frag_module, custom_uniform_names) =
        shaders.fragment_module_with_names(device, frag_src, node_label)?;

    // ── Bind group layout (same for both families) ───────────────────────
    let bind_group_layout = build_bind_group_layout(device, node_label);

    // ── Pipeline layout ──────────────────────────────────────────────────
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label:                Some(&format!("{node_label}_layout")),
        bind_group_layouts:   &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    // ── Vertex buffer layout (geometry only) ─────────────────────────────
    // Two f32 per vertex: [x, y] in NDC space.
    let vertex_buffer_layout = wgpu::VertexBufferLayout {
        array_stride: (2 * std::mem::size_of::<f32>()) as wgpu::BufferAddress,
        step_mode:    wgpu::VertexStepMode::Vertex,
        attributes:   &[wgpu::VertexAttribute {
            format:          wgpu::VertexFormat::Float32x2,
            offset:          0,
            shader_location: 0,  // @location(0) position: vec2<f32>
        }],
    };

    let vertex_buffers: &[wgpu::VertexBufferLayout] = if topology.is_geometry() {
        std::slice::from_ref(&vertex_buffer_layout)
    } else {
        &[]  // fullscreen — no vertex buffer
    };

    // ── Render pipeline ──────────────────────────────────────────────────
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label:  Some(&format!("{node_label}_pipeline")),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module:               &vert_module,
            entry_point:          Some("vs_main"),
            compilation_options:  wgpu::PipelineCompilationOptions::default(),
            buffers:              vertex_buffers,
        },
        fragment: Some(wgpu::FragmentState {
            module:              frag_module,
            entry_point:         Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format:     RENDER_TARGET_FORMAT,
                blend:      None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology:   topology.to_wgpu(),
            front_face: wgpu::FrontFace::Ccw,
            cull_mode:  None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count:                     sample_count,
            mask:                      !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache:     None,
    });

    Ok(NodePipeline { pipeline, bind_group_layout, custom_uniform_names, topology })
}

// ── Bind group layout ─────────────────────────────────────────────────────

/// Build the shared bind group layout (bindings 0–7).
///
/// **All nodes share this layout.** Binding 7 (MvpBlock) is VERTEX-stage only;
/// fullscreen nodes still need it in the bind group but just upload identity.
pub fn build_bind_group_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label:   Some(&format!("{label}_bgl")),
        entries: &[
            // 0–3: iChannel textures (fragment)
            tex_entry(0),
            tex_entry(1),
            tex_entry(2),
            tex_entry(3),
            // 4: shared sampler (fragment)
            wgpu::BindGroupLayoutEntry {
                binding:    4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            // 5: FrameBlock (fragment)
            uniform_entry(5, wgpu::ShaderStages::FRAGMENT),
            // 6: CustomBlock — u_* params (fragment)
            uniform_entry(6, wgpu::ShaderStages::FRAGMENT),
            // 7: MvpBlock — geometry matrix (vertex only)
            uniform_entry(7, wgpu::ShaderStages::VERTEX),
            // 8: iAudio_tex — audio/FFT spectrum (fragment, 2D height=1)
            tex_entry(8),
        ],
    })
}

fn tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type:    wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled:   false,
        },
        count: None,
    }
}

fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty:                 wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size:   None,
        },
        count: None,
    }
}
