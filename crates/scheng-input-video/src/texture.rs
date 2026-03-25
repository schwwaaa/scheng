//! `texture.rs` — wgpu texture for video frames.
//!
//! `VideoTexture` owns a `wgpu::Texture` sized to the video's resolution.
//! It is created once and reused across frames — only the pixel data changes.

use crate::VideoError;

/// A wgpu RGBA8 texture that holds one video frame.
pub struct VideoTexture {
    pub texture: std::sync::Arc<wgpu::Texture>,
    pub width:   u32,
    pub height:  u32,
}

impl VideoTexture {
    /// Allocate a texture at the given resolution.
    pub fn new(device: &wgpu::Device, width: u32, height: u32, label: &str) -> Self {
        let texture = std::sync::Arc::new(device.create_texture(&wgpu::TextureDescriptor {
            label:           Some(label),
            size:            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count:    1,
            dimension:       wgpu::TextureDimension::D2,
            format:          wgpu::TextureFormat::Rgba8Unorm,
            usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats:    &[],
        }));
        Self { texture, width, height }
    }

    /// Upload raw RGBA8 pixels to the texture.
    ///
    /// `pixels` must be exactly `width × height × 4` bytes.
    pub fn upload(&self, queue: &wgpu::Queue, pixels: &[u8]) {
        debug_assert_eq!(
            pixels.len(),
            (self.width * self.height * 4) as usize,
            "pixel buffer size mismatch"
        );
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture:   &*self.texture,
                mip_level: 0,
                origin:    wgpu::Origin3d::ZERO,
                aspect:    wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::ImageDataLayout {
                offset:         0,
                bytes_per_row:  Some(self.width * 4),
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );
    }

    /// Create a texture view for binding as iChannel0.
    pub fn view(&self) -> wgpu::TextureView {
        self.texture.as_ref().create_view(&wgpu::TextureViewDescriptor::default())
    }

    /// Resize the texture if resolution changed. Returns true if reallocated.
    pub fn ensure_size(&mut self, device: &wgpu::Device, width: u32, height: u32, label: &str) -> bool {
        if self.width == width && self.height == height { return false; }
        *self = Self::new(device, width, height, label);
        true
    }
}
