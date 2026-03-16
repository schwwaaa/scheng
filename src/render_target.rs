//! `render_target.rs` — per-node offscreen render textures.
//!
//! Each non-Output node in the graph gets a [`RenderTarget`]: a wgpu `Texture`
//! that the node renders into, and which downstream nodes read from as `iChannelN`.
//!
//! The texture format is always `Rgba8Unorm` — 8-bit RGBA, the same as
//! shadecore's OpenGL FBOs. This is universally supported and compatible
//! with Syphon, Spout, NDI, and FFmpeg.
//!
//! Resolution tracks `FrameCtx.width/height`. If the resolution changes
//! between frames, `RenderTarget::ensure_size` recreates the texture.

use crate::WgpuError;

/// Fixed texture format used throughout scheng's wgpu pipeline.
///
/// `Rgba8Unorm` = 4 bytes per pixel, linear [0,1] range, universally supported.
/// Matches shadecore's OpenGL FBO format for maximum compatibility.
pub const RENDER_TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// An offscreen render target for a single graph node.
///
/// Contains:
/// - the wgpu [`wgpu::Texture`] (written by the node's shader)
/// - a [`wgpu::TextureView`] for use as a render attachment
/// - a second [`wgpu::TextureView`] for use as a shader-readable texture (iChannelN)
///
/// Both views point to the same underlying texture; wgpu allows this.
pub struct RenderTarget {
    /// The GPU texture.
    pub texture: wgpu::Texture,
    /// View used as the render pass color attachment (write target).
    pub render_view: wgpu::TextureView,
    /// View used as a sampled texture in downstream shaders (read target).
    pub sample_view: wgpu::TextureView,
    /// Cached width (pixels).
    pub width: u32,
    /// Cached height (pixels).
    pub height: u32,
}

impl RenderTarget {
    /// Create a new render target at the given resolution.
    pub fn new(device: &wgpu::Device, width: u32, height: u32, label: &str) -> Self {
        let (texture, render_view, sample_view) =
            create_texture(device, width, height, label);
        Self { texture, render_view, sample_view, width, height }
    }

    /// Ensure the render target matches the given resolution.
    ///
    /// If the resolution matches the current texture, this is a no-op.
    /// Otherwise the texture is recreated (cheap — just GPU memory allocation).
    pub fn ensure_size(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        label: &str,
    ) {
        if self.width == width && self.height == height {
            return;
        }
        log::debug!("RenderTarget '{}': resizing {}×{} → {}×{}", label, self.width, self.height, width, height);
        let (texture, render_view, sample_view) = create_texture(device, width, height, label);
        self.texture    = texture;
        self.render_view = render_view;
        self.sample_view = sample_view;
        self.width      = width;
        self.height     = height;
    }

    /// Read back the rendered pixels from this target to the CPU.
    ///
    /// Returns raw RGBA bytes, row-major, from top-left to bottom-right
    /// (wgpu / Vulkan convention — note Y is flipped vs OpenGL).
    ///
    /// This creates a staging buffer, submits a copy, polls until done,
    /// and returns the data. It is synchronous and intended for testing only.
    /// **Do not call this in the hot render loop.**
    pub fn readback(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<u8> {
        // wgpu requires bytes_per_row to be aligned to COPY_BYTES_PER_ROW_ALIGNMENT (256).
        let unaligned = 4 * self.width;
        let aligned = align_to(unaligned, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let total_bytes = (aligned * self.height) as u64;

        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback_staging"),
            size: total_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        // Submit copy: texture → staging buffer.
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("readback_encoder"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &staging,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(aligned),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(std::iter::once(encoder.finish()));

        // Map the staging buffer synchronously.
        staging.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::Maintain::Wait);

        // Strip row padding and return tightly-packed RGBA pixels.
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

// ── Blank texture (1×1 black) ─────────────────────────────────────────────

/// Create a 1×1 black RGBA texture used for unconnected `iChannelN` inputs.
///
/// This avoids undefined sampler behaviour when a shader reads from a channel
/// that has no upstream node connected.
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
    // Write [0, 0, 0, 255] — fully transparent black.
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[0u8, 0, 0, 255],
        wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: Some(1) },
        wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
    );
    texture
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn create_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    label: &str,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: RENDER_TARGET_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC, // needed for readback
        view_formats: &[],
    });
    // Both views are identical — wgpu allows the same texture as both
    // render attachment and sample source (in separate passes).
    let render_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sample_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, render_view, sample_view)
}

fn align_to(value: u32, alignment: u32) -> u32 {
    (value + alignment - 1) & !(alignment - 1)
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_to_256() {
        assert_eq!(align_to(0, 256), 0);
        assert_eq!(align_to(1, 256), 256);
        assert_eq!(align_to(256, 256), 256);
        assert_eq!(align_to(257, 256), 512);
        assert_eq!(align_to(512, 256), 512);
        // Common case: 4 bytes/pixel × 640 pixels = 2560 → already aligned
        assert_eq!(align_to(4 * 640, 256), 4 * 640);
        // 4 × 16 = 64, next multiple of 256 = 256
        assert_eq!(align_to(4 * 16, 256), 256);
    }
}
