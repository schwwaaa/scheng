//! `render_target.rs` — per-node offscreen render textures with MSAA.
//!
//! Each non-Output node gets a RenderTarget: a single-sample resolve texture
//! (readable by downstream nodes) plus an optional MSAA texture (rendered into,
//! then resolved each frame).
//!
//! MSAA sample count is configurable: 1 (off), 2, 4 (default), 8.
//! Not all backends support all counts — Metal supports 1, 2, 4, 8.

/// Fixed texture format used throughout scheng's wgpu pipeline.
pub const RENDER_TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Default MSAA sample count.
/// 1 = off (default, maximum compatibility).
/// Set to 4 via --msaa 4 for anti-aliasing (Metal/DX12/Vulkan all support 4x).
pub const DEFAULT_SAMPLE_COUNT: u32 = 1;

/// An offscreen render target for a single graph node.
pub struct RenderTarget {
    /// Resolved single-sample texture (used as iChannelN by downstream nodes).
    pub texture:      wgpu::Texture,
    /// View of the resolved texture for use as render attachment (sample_count=1).
    pub render_view:  wgpu::TextureView,
    /// View of the resolved texture for sampling in shaders.
    pub sample_view:  wgpu::TextureView,
    /// MSAA multisample texture (rendered into, resolved each frame).
    /// None when sample_count == 1.
    pub msaa_texture: Option<wgpu::Texture>,
    /// View of the MSAA texture used as render attachment.
    pub msaa_view:    Option<wgpu::TextureView>,
    pub width:        u32,
    pub height:       u32,
    pub sample_count: u32,
}

impl RenderTarget {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, label: &str) -> Self {
        Self::new_with_samples(device, width, height, label, DEFAULT_SAMPLE_COUNT)
    }

    pub fn new_with_samples(
        device:       &wgpu::Device,
        width:        u32,
        height:       u32,
        label:        &str,
        sample_count: u32,
    ) -> Self {
        let (texture, render_view, sample_view, msaa_texture, msaa_view) =
            create_textures(device, width, height, label, sample_count);
        Self { texture, render_view, sample_view, msaa_texture, msaa_view, width, height, sample_count }
    }

    pub fn ensure_size(&mut self, device: &wgpu::Device, width: u32, height: u32, label: &str) {
        if self.width == width && self.height == height { return; }
        log::debug!("RenderTarget '{label}': resizing {}×{} → {}×{}", self.width, self.height, width, height);
        let (texture, render_view, sample_view, msaa_texture, msaa_view) =
            create_textures(device, width, height, label, self.sample_count);
        self.texture     = texture;
        self.render_view = render_view;
        self.sample_view = sample_view;
        self.msaa_texture = msaa_texture;
        self.msaa_view    = msaa_view;
        self.width  = width;
        self.height = height;
    }

    /// The view to use as the render pass color attachment.
    /// When MSAA is active this is the multisample texture.
    /// When MSAA is off this is the resolve texture directly.
    pub fn attachment_view(&self) -> &wgpu::TextureView {
        self.msaa_view.as_ref().unwrap_or(&self.render_view)
    }

    /// The resolve target for the render pass.
    /// Some(render_view) when MSAA is active, None when sample_count == 1.
    pub fn resolve_target(&self) -> Option<&wgpu::TextureView> {
        if self.msaa_view.is_some() { Some(&self.render_view) } else { None }
    }

    pub fn readback(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<u8> {
        let unaligned = 4 * self.width;
        let aligned = align_to(unaligned, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let total_bytes = (aligned * self.height) as u64;

        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback_staging"),
            size: total_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("readback_encoder") }
        );
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &self.texture, mip_level: 0,
                origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &staging,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(aligned),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );
        queue.submit(std::iter::once(encoder.finish()));

        staging.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::Maintain::Wait);

        let mapped = staging.slice(..).get_mapped_range();
        let mut pixels = Vec::with_capacity((4 * self.width * self.height) as usize);
        for row in 0..self.height {
            let start = (row * aligned) as usize;
            let end   = start + (4 * self.width) as usize;
            pixels.extend_from_slice(&mapped[start..end]);
        }
        drop(mapped);
        staging.unmap();
        pixels
    }
}

pub fn create_blank_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("blank_1x1"),
        size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: RENDER_TARGET_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture, mip_level: 0,
            origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All,
        },
        &[0u8, 0, 0, 255],
        wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: Some(1) },
        wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
    );
    texture
}

fn create_textures(
    device:       &wgpu::Device,
    width:        u32,
    height:       u32,
    label:        &str,
    sample_count: u32,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::TextureView,
      Option<wgpu::Texture>, Option<wgpu::TextureView>)
{
    // Resolve texture — always sample_count=1, readable by shaders + copyable
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: RENDER_TARGET_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let render_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sample_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    // MSAA texture — only when sample_count > 1
    let (msaa_texture, msaa_view) = if sample_count > 1 {
        let msaa = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("{label}_msaa")),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: RENDER_TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
        (Some(msaa), Some(view))
    } else {
        (None, None)
    };

    (texture, render_view, sample_view, msaa_texture, msaa_view)
}

fn align_to(value: u32, alignment: u32) -> u32 {
    (value + alignment - 1) & !(alignment - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_to_256() {
        assert_eq!(align_to(0, 256), 0);
        assert_eq!(align_to(1, 256), 256);
        assert_eq!(align_to(256, 256), 256);
        assert_eq!(align_to(257, 256), 512);
        assert_eq!(align_to(4 * 640, 256), 4 * 640);
        assert_eq!(align_to(4 * 16, 256), 256);
    }
}
