//! `temporal_ring.rs` — N-frame ring buffer of wgpu render targets.
//!
//! Ports `scheng-buffers/TemporalRing` from the glow backend to wgpu.
//!
//! # What it does
//!
//! Keeps a circular buffer of N rendered frames. Each frame, the instrument
//! pushes the current output into the ring. Shaders can then sample any
//! prior frame by binding `ring.texture_frames_ago(n)` into
//! `NodeConfig::input_textures`.
//!
//! # Use cases
//!
//! - Motion blur (average frames 0–5)
//! - Variable-delay feedback (sample frame N ago instead of just frame N-1)
//! - Slit-scan (sample one row from each of the last N frames)
//! - Echo / trails with configurable decay depth
//!
//! # Usage
//!
//! ```rust,ignore
//! use scheng_runtime_wgpu::temporal_ring::TemporalRing;
//!
//! // Create at startup — 8 frames of history at 1280×720
//! let mut ring = TemporalRing::new(&runtime.ctx.device, 1280, 720, 8);
//!
//! // Each frame, after execute_frame():
//! // Push the current output texture into the ring
//! ring.push(&runtime.ctx.device, &runtime.ctx.queue, &current_render_target);
//!
//! // Bind "2 frames ago" into a node's iChannel1
//! if let Some(tex) = ring.texture_frames_ago(2) {
//!     node_config.input_textures[1] = Some(tex);
//! }
//! ```

use std::sync::Arc;
use crate::render_target::{RenderTarget, RENDER_TARGET_FORMAT};

/// A circular buffer of N wgpu render targets for temporal frame sampling.
pub struct TemporalRing {
    slots:    Vec<Arc<wgpu::Texture>>,
    width:    u32,
    height:   u32,
    /// Next write position. After push(), head advances by 1.
    head:     usize,
    /// Number of frames successfully pushed. Capped at capacity.
    /// Used to avoid returning stale/uninitialised slots before the ring fills.
    filled:   usize,
}

impl TemporalRing {
    /// Create a ring of `capacity` slots at the given resolution.
    ///
    /// All slots are initialised to opaque black so sampling before the
    /// first push() is safe and returns black rather than undefined data.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue,
               width: u32, height: u32, capacity: usize) -> Self {
        let cap = capacity.max(1);
        let black = vec![0u8; (width * height * 8) as usize]; // Rgba16Float = 8 bytes/px

        let slots = (0..cap).map(|i| {
            let tex = Arc::new(device.create_texture(&wgpu::TextureDescriptor {
                label:           Some(&format!("temporal_ring_slot_{i}")),
                size:            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count:    1,
                dimension:       wgpu::TextureDimension::D2,
                format:          RENDER_TARGET_FORMAT,
                usage:           wgpu::TextureUsages::TEXTURE_BINDING
                                 | wgpu::TextureUsages::COPY_DST
                                 | wgpu::TextureUsages::COPY_SRC,
                view_formats:    &[],
            }));
            // Initialise to black
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &tex, mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &black,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row:  Some(width * 8),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            );
            tex
        }).collect();

        Self { slots, width, height, head: 0, filled: 0 }
    }

    /// Copy `source` (a completed render target) into the next ring slot.
    ///
    /// Call once per frame, after `WgpuRuntime::execute_frame()` and
    /// after `queue.submit()` so the source texture is fully written.
    ///
    /// If the source dimensions differ from the ring dimensions, the copy
    /// is skipped and a warning is logged. Resize the ring with `resize()`
    /// if the render resolution changes.
    pub fn push(&mut self, device: &wgpu::Device, queue: &wgpu::Queue,
                source: &RenderTarget) {
        if source.width != self.width || source.height != self.height {
            log::warn!(
                "TemporalRing::push: source {}×{} != ring {}×{} — skipping",
                source.width, source.height, self.width, self.height
            );
            return;
        }

        let dst = &self.slots[self.head];

        let mut encoder = device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("temporal_ring_push") }
        );
        encoder.copy_texture_to_texture(
            wgpu::ImageCopyTexture {
                texture:   &source.texture,
                mip_level: 0,
                origin:    wgpu::Origin3d::ZERO,
                aspect:    wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyTexture {
                texture:   dst,
                mip_level: 0,
                origin:    wgpu::Origin3d::ZERO,
                aspect:    wgpu::TextureAspect::All,
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );
        queue.submit(std::iter::once(encoder.finish()));

        self.head = (self.head + 1) % self.slots.len();
        self.filled = (self.filled + 1).min(self.slots.len());
    }

    /// Returns an `Arc<wgpu::Texture>` for the frame `n` frames ago.
    ///
    /// - `frames_ago = 0` → the most recently pushed frame
    /// - `frames_ago = 1` → the frame before that
    /// - etc.
    ///
    /// Returns `None` if fewer than `frames_ago + 1` frames have been pushed
    /// (i.e. the ring hasn't filled yet). This prevents sampling uninitialised
    /// black frames as if they were real content.
    ///
    /// After the ring is full, always returns `Some`.
    pub fn texture_frames_ago(&self, frames_ago: usize) -> Option<Arc<wgpu::Texture>> {
        if frames_ago >= self.filled {
            return None;
        }
        let n    = self.slots.len();
        // head points to the NEXT write slot, so head-1 is the newest pushed frame.
        let idx  = (self.head + n - 1 - (frames_ago % n)) % n;
        Some(Arc::clone(&self.slots[idx]))
    }

    /// Resize the ring to a new resolution. All slots are re-allocated and
    /// cleared to black. `filled` resets to 0 — the ring must refill.
    ///
    /// Call when the render resolution changes (e.g. window resize).
    pub fn resize(&mut self, device: &wgpu::Device, queue: &wgpu::Queue,
                  width: u32, height: u32) {
        *self = Self::new(device, queue, width, height, self.slots.len());
    }

    pub fn capacity(&self) -> usize { self.slots.len() }
    pub fn width(&self)    -> u32   { self.width }
    pub fn height(&self)   -> u32   { self.height }

    /// How many frames have been pushed since creation or last resize.
    /// Reaches `capacity()` once the ring is full.
    pub fn frames_pushed(&self) -> usize { self.filled }
    pub fn is_full(&self)       -> bool  { self.filled >= self.slots.len() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_index_math() {
        // Simulate the index arithmetic without a GPU.
        // Ring capacity 4, push 6 times → head = 2, filled = 4.
        let cap = 4usize;
        let mut head   = 0usize;
        let mut filled = 0usize;

        for _ in 0..6 {
            head   = (head + 1) % cap;
            filled = (filled + 1).min(cap);
        }
        // head = 2, filled = 4 (full)
        assert_eq!(head,   2);
        assert_eq!(filled, 4);

        // frames_ago=0 → slot (2+4-1-0)%4 = 1  (last written)
        // frames_ago=1 → slot (2+4-1-1)%4 = 0
        // frames_ago=2 → slot (2+4-1-2)%4 = 3
        // frames_ago=3 → slot (2+4-1-3)%4 = 2
        let idx = |ago: usize| (head + cap - 1 - (ago % cap)) % cap;
        assert_eq!(idx(0), 1);
        assert_eq!(idx(1), 0);
        assert_eq!(idx(2), 3);
        assert_eq!(idx(3), 2);
    }

    #[test]
    fn not_full_returns_none_for_deep_frames() {
        // filled=2, cap=4 → only frames_ago 0 and 1 are valid
        let filled = 2usize;
        let cap    = 4usize;

        let valid = |ago: usize| ago < filled.min(cap);
        assert!( valid(0));
        assert!( valid(1));
        assert!(!valid(2));
        assert!(!valid(3));
    }
}
