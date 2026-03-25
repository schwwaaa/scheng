//! `uniforms.rs` — GPU uniform buffers.
//!
//! Two uniform buffers per-frame:
//! - Binding 5: `FrameBlock`  — global (uTime, uResolution, uFrame)
//! - Binding 6: `CustomBlock` — per-node (u_custom[32], written per draw call)

use bytemuck::{Pod, Zeroable};
use crate::FrameCtx;

pub const MAX_CUSTOM_UNIFORMS: usize = crate::compat::MAX_CUSTOM_UNIFORMS;

// ── FrameBlock (binding 5) ────────────────────────────────────────────────

/// GPU mirror of `FrameBlock` — global per-frame uniforms.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct FrameUniforms {
    pub resolution: [f32; 2],
    pub time:       f32,
    pub frame:      u32,
}

impl FrameUniforms {
    pub fn from_ctx(ctx: &FrameCtx) -> Self {
        Self {
            resolution: [ctx.width as f32, ctx.height as f32],
            time:       ctx.time,
            frame:      ctx.frame as u32,
        }
    }
    pub fn as_bytes(&self) -> &[u8] { bytemuck::bytes_of(self) }
}

pub struct UniformManager {
    pub buffer: wgpu::Buffer,
}

impl UniformManager {
    pub fn new(device: &wgpu::Device) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("scheng_frame_uniforms"),
            size:               std::mem::size_of::<FrameUniforms>() as u64,
            usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { buffer }
    }

    pub fn update(&self, queue: &wgpu::Queue, ctx: &FrameCtx) {
        queue.write_buffer(&self.buffer, 0, FrameUniforms::from_ctx(ctx).as_bytes());
    }
}

// ── CustomBlock (binding 6) ───────────────────────────────────────────────

/// GPU mirror of `CustomBlock` — per-node custom u_* uniforms.
/// Fixed-size array of f32 matching MAX_CUSTOM_UNIFORMS.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct CustomUniforms {
    pub values: [f32; MAX_CUSTOM_UNIFORMS],
}

impl CustomUniforms {
    pub fn zeroed() -> Self {
        Self { values: [0.0f32; MAX_CUSTOM_UNIFORMS] }
    }

    pub fn as_bytes(&self) -> &[u8] { bytemuck::bytes_of(self) }
}

/// Per-node uniform buffer for custom u_* values.
/// One per node in the graph — created lazily, reused across frames.
pub struct CustomUniformBuffer {
    pub buffer: wgpu::Buffer,
}

impl CustomUniformBuffer {
    pub fn new(device: &wgpu::Device, label: &str) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some(label),
            size:               std::mem::size_of::<CustomUniforms>() as u64,
            usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { buffer }
    }

    /// Write custom uniform values for this node.
    /// `names` — the u_* names in declaration order (from ProcessedShader).
    /// `values` — the NodeConfig::uniforms map.
    pub fn update(
        &self,
        queue:  &wgpu::Queue,
        names:  &[String],
        values: &std::collections::HashMap<String, f32>,
    ) {
        let mut cu = CustomUniforms::zeroed();
        for (idx, name) in names.iter().enumerate().take(MAX_CUSTOM_UNIFORMS) {
            cu.values[idx] = *values.get(name).unwrap_or(&0.0);
        }
        queue.write_buffer(&self.buffer, 0, cu.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn frame_uniforms_size_is_16_bytes() {
        assert_eq!(size_of::<FrameUniforms>(), 16);
    }

    #[test]
    fn custom_uniforms_size() {
        // 32 × 4 bytes = 128 bytes
        assert_eq!(size_of::<CustomUniforms>(), MAX_CUSTOM_UNIFORMS * 4);
    }

    #[test]
    fn custom_uniforms_update() {
        let mut map = std::collections::HashMap::new();
        map.insert("u_brightness".to_owned(), 0.75f32);
        map.insert("u_contrast".to_owned(),   1.5f32);

        let names = vec!["u_brightness".to_owned(), "u_contrast".to_owned()];
        let mut cu = CustomUniforms::zeroed();
        for (idx, name) in names.iter().enumerate().take(MAX_CUSTOM_UNIFORMS) {
            cu.values[idx] = *map.get(name).unwrap_or(&0.0);
        }
        assert!((cu.values[0] - 0.75).abs() < 1e-6);
        assert!((cu.values[1] - 1.5 ).abs() < 1e-6);
        assert_eq!(cu.values[2], 0.0);
    }

    #[test]
    fn frame_uniforms_from_ctx() {
        let ctx = FrameCtx { width: 1280, height: 720, time: 1.0, frame: 10 };
        let u = FrameUniforms::from_ctx(&ctx);
        assert_eq!(u.resolution, [1280.0, 720.0]);
        assert_eq!(u.time, 1.0);
        assert_eq!(u.frame, 10);
    }
}
