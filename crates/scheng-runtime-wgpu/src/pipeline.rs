//! `pipeline.rs` — RenderPipeline and BindGroupLayout cache.
//!
//! Bind group layout (bindings 0–6):
//! 0–3 = iChannel0..3 textures
//!   4 = iSampler
//!   5 = FrameBlock  (global uniforms)
//!   6 = CustomBlock (per-node u_* uniforms) ← Phase 1.2

use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};

use crate::{render_target::RENDER_TARGET_FORMAT, shader::{ShaderCache, VERTEX_SHADER_WGSL}, WgpuError};

pub struct NodePipeline {
    pub pipeline:          wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
    /// Custom uniform names in declaration order — used to write CustomBlock.
    pub custom_uniform_names: Vec<String>,
}

pub struct PipelineCache {
    pipelines: HashMap<u64, NodePipeline>,
    shaders:   ShaderCache,
}

impl PipelineCache {
    pub fn new() -> Self {
        Self { pipelines: HashMap::new(), shaders: ShaderCache::new() }
    }

    pub fn get_or_create<'a>(
        &'a mut self,
        device:     &wgpu::Device,
        frag_src:   &str,
        node_label: &str,
    ) -> Result<&'a NodePipeline, WgpuError> {
        let mut h = DefaultHasher::new();
        frag_src.hash(&mut h);
        let hash = h.finish();

        if !self.pipelines.contains_key(&hash) {
            let p = build_pipeline(device, &mut self.shaders, frag_src, node_label)?;
            self.pipelines.insert(hash, p);
        }
        Ok(self.pipelines.get(&hash).unwrap())
    }

    pub fn clear(&mut self) {
        self.pipelines.clear();
        self.shaders.clear();
    }
}

fn build_pipeline(
    device:     &wgpu::Device,
    shaders:    &mut ShaderCache,
    frag_src:   &str,
    node_label: &str,
) -> Result<NodePipeline, WgpuError> {
    let vert_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label:  Some("scheng_vert"),
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(VERTEX_SHADER_WGSL)),
    });

    // Compile frag — also extracts custom_uniform_names
    let (frag_module, custom_uniform_names) =
        shaders.fragment_module_with_names(device, frag_src, node_label)?;

    let bgl    = build_bgl(device, node_label);
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label:                Some(&format!("{node_label}_layout")),
        bind_group_layouts:   &[&bgl],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label:  Some(&format!("{node_label}_pipeline")),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module:              &vert_module,
            entry_point:         Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers:             &[],
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
            topology:  wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample:   wgpu::MultisampleState::default(),
        multiview:     None,
        cache:         None,
    });

    Ok(NodePipeline { pipeline, bind_group_layout: bgl, custom_uniform_names })
}

fn build_bgl(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label:   Some(&format!("{label}_bgl")),
        entries: &[
            tex(0), tex(1), tex(2), tex(3),
            // binding 4 — iSampler
            wgpu::BindGroupLayoutEntry {
                binding:    4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty:         wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count:      None,
            },
            // binding 5 — FrameBlock
            uniform_entry(5),
            // binding 6 — CustomBlock (per-node u_* uniforms)
            uniform_entry(6),
        ],
    })
}

fn tex(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty:         wgpu::BindingType::Texture {
            sample_type:    wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled:   false,
        },
        count: None,
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty:         wgpu::BindingType::Buffer {
            ty:                 wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size:   None,
        },
        count: None,
    }
}
