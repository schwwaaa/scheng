//! `preview.rs` — PreviewSink: blits the render target to a winit window.
//!
//! # Critical: shared device
//!
//! The wgpu surface and WgpuRuntime MUST use the same Device.
//! Use `PreviewSink::create_surface()` to create the surface BEFORE
//! creating WgpuRuntime, then pass the surface to `WgpuRuntime::new_for_surface()`.
//! Finally call `PreviewSink::new()` with the now-configured surface and
//! the runtime's device/queue/adapter.

use scheng_graph::NodeId;
use scheng_runtime_wgpu::{executor::OutputSink, FrameCtx, RenderTarget};
use winit::window::Window;

const BLIT_VERT: &str = r#"
struct Out { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@builtin(vertex_index) i: u32) -> Out {
    var p = array<vec2<f32>,3>(vec2(-1.,-1.), vec2(3.,-1.), vec2(-1.,3.));
    var u = array<vec2<f32>,3>(vec2(0.,1.), vec2(2.,1.), vec2(0.,-1.));
    var o: Out; o.pos = vec4(p[i],0.,1.); o.uv = u[i]; return o;
}
"#;

const BLIT_FRAG: &str = r#"
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@fragment fn fs(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    return textureSample(t, s, uv);
}
"#;

pub struct PreviewSink {
    surface:        wgpu::Surface<'static>,
    config:         wgpu::SurfaceConfiguration,
    pipeline:       wgpu::RenderPipeline,
    bgl:            wgpu::BindGroupLayout,
    sampler:        wgpu::Sampler,
    target_node:    NodeId,
}

impl PreviewSink {
    /// Step 1: create the surface before WgpuRuntime.
    /// Pass the returned surface to `WgpuRuntime::new_for_surface()`.
    pub fn create_surface(
        window:   &'static Window,
        instance: &wgpu::Instance,
    ) -> wgpu::Surface<'static> {
        instance.create_surface(window).expect("Surface creation failed")
    }

    /// Step 2: after WgpuRuntime is created with `new_for_surface()`,
    /// call this to finish setting up the sink.
    pub fn new(
        surface:     wgpu::Surface<'static>,
        device:      &wgpu::Device,
        queue:       &wgpu::Queue,
        adapter:     &wgpu::Adapter,
        target_node: NodeId,
        width:       u32,
        height:      u32,
    ) -> Self {
        let caps   = surface.get_capabilities(adapter);
        let format = caps.formats.iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage:        wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode:   wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(device, &config);

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("blit_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type:    wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled:   false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit_layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let vert = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit_vert"), source: wgpu::ShaderSource::Wgsl(BLIT_VERT.into()),
        });
        let frag = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit_frag"), source: wgpu::ShaderSource::Wgsl(BLIT_FRAG.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("blit_pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &vert, entry_point: Some("vs"),
                compilation_options: Default::default(), buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &frag, entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend:      Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive:     wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            multiview:     None,
            cache:         None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let _ = queue;
        Self { surface, config, pipeline, bgl, sampler, target_node }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == 0 || height == 0 { return; }
        self.config.width  = width;
        self.config.height = height;
        self.surface.configure(device, &self.config);
    }
}

impl OutputSink for PreviewSink {
    fn present(&mut self, node_id: NodeId, target: &RenderTarget, _ctx: &FrameCtx,
               device: &wgpu::Device, queue: &wgpu::Queue) {
        if node_id != self.target_node { return; }

        let Ok(frame) = self.surface.get_current_texture() else { return };
        let frame_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let src_view   = target.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:  Some("blit_bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&src_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });

        let mut enc = device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("blit") }
        );
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &frame_view, resolve_target: None,
                    ops: wgpu::Operations {
                        load:  wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes:         None,
                occlusion_query_set:      None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit(std::iter::once(enc.finish()));
        frame.present();
    }
}
