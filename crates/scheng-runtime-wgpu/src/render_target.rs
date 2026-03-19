//! Per-node offscreen render textures.

pub const RENDER_TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub struct RenderTarget {
    pub texture:      wgpu::Texture,
    pub render_view:  wgpu::TextureView,
    pub sample_view:  wgpu::TextureView,
    pub width:        u32,
    pub height:       u32,
}

impl RenderTarget {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, label: &str) -> Self {
        let (texture, render_view, sample_view) = make_texture(device, width, height, label);
        Self { texture, render_view, sample_view, width, height }
    }

    pub fn ensure_size(&mut self, device: &wgpu::Device, width: u32, height: u32, label: &str) {
        if self.width == width && self.height == height { return; }
        let (texture, render_view, sample_view) = make_texture(device, width, height, label);
        self.texture     = texture;
        self.render_view = render_view;
        self.sample_view = sample_view;
        self.width  = width;
        self.height = height;
    }

    /// Read pixels back to CPU (testing only — synchronous and expensive).
    pub fn readback(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<u8> {
        let unaligned = 4 * self.width;
        let aligned   = align_to(unaligned, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let total     = (aligned * self.height) as u64;

        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("readback_staging"),
            size:               total,
            usage:              wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut enc = device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("readback_enc") }
        );
        enc.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture:   &self.texture,
                mip_level: 0,
                origin:    wgpu::Origin3d::ZERO,
                aspect:    wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &staging,
                layout: wgpu::ImageDataLayout {
                    offset:         0,
                    bytes_per_row:  Some(aligned),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );
        queue.submit(std::iter::once(enc.finish()));

        staging.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::Maintain::Wait);

        let mapped = staging.slice(..).get_mapped_range();
        let mut out = Vec::with_capacity((4 * self.width * self.height) as usize);
        for row in 0..self.height {
            let start = (row * aligned) as usize;
            out.extend_from_slice(&mapped[start .. start + (4 * self.width) as usize]);
        }
        drop(mapped);
        staging.unmap();
        out
    }
}

pub fn create_blank_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label:           Some("blank_1x1"),
        size:            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count:    1,
        dimension:       wgpu::TextureDimension::D2,
        format:          RENDER_TARGET_FORMAT,
        usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats:    &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &tex, mip_level: 0,
            origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All,
        },
        &[0u8, 0, 0, 255],
        wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: Some(1) },
        wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
    );
    tex
}

fn make_texture(
    device: &wgpu::Device, width: u32, height: u32, label: &str
) -> (wgpu::Texture, wgpu::TextureView, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label:           Some(label),
        size:            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count:    1,
        dimension:       wgpu::TextureDimension::D2,
        format:          RENDER_TARGET_FORMAT,
        usage:           wgpu::TextureUsages::RENDER_ATTACHMENT
                       | wgpu::TextureUsages::TEXTURE_BINDING
                       | wgpu::TextureUsages::COPY_SRC,
        view_formats:    &[],
    });
    let rv = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let sv = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, rv, sv)
}

fn align_to(val: u32, align: u32) -> u32 {
    (val + align - 1) & !(align - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn align_to_256() {
        assert_eq!(align_to(1,   256), 256);
        assert_eq!(align_to(256, 256), 256);
        assert_eq!(align_to(257, 256), 512);
        assert_eq!(align_to(4 * 640, 256), 4 * 640);
    }
}

// ── PingPongTarget ────────────────────────────────────────────────────────

/// Two render targets that alternate roles each frame.
///
/// Used by `Feedback` and `PreviousFrame` nodes to provide the previous
/// frame's texture as input while rendering into a fresh target.
///
/// # Frame N:
///   - render INTO `targets[write_idx]`
///   - sample FROM `targets[read_idx]`  (previous frame result)
///
/// # Frame N+1: call `swap()` — write_idx and read_idx exchange.
///
/// Initialized to black on creation so frame 0 reads a valid (empty) texture.
pub struct PingPongTarget {
    targets:   [RenderTarget; 2],
    write_idx: usize,
}

impl PingPongTarget {
    /// Create two render targets initialized to black.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32, label: &str) -> Self {
        let a = RenderTarget::new(device, width, height, &format!("{label}_ping"));
        let b = RenderTarget::new(device, width, height, &format!("{label}_pong"));
        // Clear both to black so frame 0 reads a valid texture
        clear_to_black(device, queue, &a);
        clear_to_black(device, queue, &b);
        Self { targets: [a, b], write_idx: 0 }
    }

    /// The target to render INTO this frame.
    pub fn write_target(&self) -> &RenderTarget {
        &self.targets[self.write_idx]
    }

    /// The texture to sample FROM this frame (previous frame's result).
    pub fn read_texture_view(&self) -> wgpu::TextureView {
        let read_idx = 1 - self.write_idx;
        self.targets[read_idx].texture
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

    /// Swap ping and pong. Call after submitting the frame's render commands.
    pub fn swap(&mut self) {
        self.write_idx = 1 - self.write_idx;
    }

    /// Ensure both internal targets match the given resolution.
    pub fn ensure_size(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32, label: &str) {
        if self.targets[0].width == width && self.targets[0].height == height {
            return;
        }
        self.targets[0].ensure_size(device, width, height, &format!("{label}_ping"));
        self.targets[1].ensure_size(device, width, height, &format!("{label}_pong"));
        // Re-clear after resize
        clear_to_black(device, queue, &self.targets[0]);
        clear_to_black(device, queue, &self.targets[1]);
    }
}

/// Clear a render target to black by uploading a zero-filled texture.
/// Used to ensure ping-pong buffers start clean on init and resize.
fn clear_to_black(_device: &wgpu::Device, queue: &wgpu::Queue, target: &RenderTarget) {
    let pixels = vec![0u8; (target.width * target.height * 4) as usize];
    let aligned = align_to(4 * target.width, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    // If width is already aligned we can write directly; otherwise build a padded buffer
    if aligned == 4 * target.width {
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture:   &target.texture,
                mip_level: 0,
                origin:    wgpu::Origin3d::ZERO,
                aspect:    wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::ImageDataLayout {
                offset:         0,
                bytes_per_row:  Some(4 * target.width),
                rows_per_image: Some(target.height),
            },
            wgpu::Extent3d { width: target.width, height: target.height, depth_or_array_layers: 1 },
        );
    }
    // For non-aligned widths the clear is skipped — not critical since black is the desired state
    // and the texture starts zeroed on GPU allocation.
    let _ = aligned;
}
