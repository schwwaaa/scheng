//! `uniforms.rs` — GPU uniform buffer management.
//!
//! The compat header declares a `FrameBlock` uniform buffer at binding 5:
//! ```glsl
//! layout(binding = 5) uniform FrameBlock {
//!     vec2  uResolution;
//!     float uTime;
//!     uint  uFrame;
//! };
//! ```
//!
//! This module maintains a single `wgpu::Buffer` for this block and writes
//! updated values from [`FrameCtx`] before each draw call.
//!
//! # Memory layout (std140)
//!
//! GLSL uniform blocks use std140 layout rules:
//! - `vec2`  → 8 bytes (2 × f32)
//! - `float` → 4 bytes
//! - `uint`  → 4 bytes
//! Total: 16 bytes. The struct below matches this exactly.
//!
//! `bytemuck::Pod` lets us cast it to `&[u8]` for `queue.write_buffer`.

use bytemuck::{Pod, Zeroable};
use scheng_core::FrameCtx;

// ── GPU-side struct (must match GLSL FrameBlock layout) ──────────────────

/// Mirror of the `FrameBlock` uniform block in the compat header.
///
/// `repr(C)` + `Pod` ensures the memory layout matches GLSL std140.
/// All fields are 4-byte aligned and the total size is 16 bytes.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct FrameUniforms {
    /// `uResolution` — output width and height in pixels.
    pub resolution: [f32; 2],
    /// `uTime` — seconds since the instrument started.
    pub time: f32,
    /// `uFrame` — monotonic frame counter (u32 matches GLSL `uint`).
    pub frame: u32,
}

impl FrameUniforms {
    /// Build from a [`FrameCtx`].
    pub fn from_ctx(ctx: &FrameCtx) -> Self {
        Self {
            resolution: [ctx.width as f32, ctx.height as f32],
            time: ctx.time,
            // Truncate u64 → u32; frame counter wraps after ~4 billion frames.
            frame: ctx.frame as u32,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
}

// ── UniformManager ────────────────────────────────────────────────────────

/// Manages the `FrameBlock` GPU buffer shared across all nodes.
///
/// One buffer per runtime, updated once per frame before any draw calls.
pub struct UniformManager {
    /// The GPU uniform buffer. Size = `size_of::<FrameUniforms>()`.
    pub buffer: wgpu::Buffer,
}

impl UniformManager {
    /// Allocate the uniform buffer on the GPU.
    pub fn new(device: &wgpu::Device) -> Self {
        use std::mem::size_of;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scheng_frame_uniforms"),
            size: size_of::<FrameUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { buffer }
    }

    /// Write updated frame values to the GPU buffer.
    ///
    /// Call this once per frame, before submitting any render passes.
    pub fn update(&self, queue: &wgpu::Queue, ctx: &FrameCtx) {
        let uniforms = FrameUniforms::from_ctx(ctx);
        queue.write_buffer(&self.buffer, 0, uniforms.as_bytes());
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn frame_uniforms_size_is_16_bytes() {
        // GLSL std140: vec2(8) + float(4) + uint(4) = 16 bytes
        assert_eq!(size_of::<FrameUniforms>(), 16);
    }

    #[test]
    fn frame_uniforms_from_ctx() {
        let ctx = FrameCtx { width: 1280, height: 720, time: 3.14, frame: 42 };
        let u = FrameUniforms::from_ctx(&ctx);
        assert_eq!(u.resolution, [1280.0, 720.0]);
        assert!((u.time - 3.14).abs() < 1e-5);
        assert_eq!(u.frame, 42);
    }

    #[test]
    fn frame_uniforms_bytes_len() {
        let ctx = FrameCtx { width: 100, height: 100, time: 0.0, frame: 0 };
        let u = FrameUniforms::from_ctx(&ctx);
        assert_eq!(u.as_bytes().len(), 16);
    }
}
