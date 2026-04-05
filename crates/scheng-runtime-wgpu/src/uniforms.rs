//! `uniforms.rs` — GPU uniform buffers.
//!
//! Three uniform buffers per draw call:
//! - Binding 5: `FrameBlock`  — global (uTime, uResolution, uFrame)
//! - Binding 6: `CustomBlock` — per-node (u_custom[16], written per draw call)
//! - Binding 7: `MvpBlock`    — per-node 4×4 matrix (identity for fullscreen nodes)

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

// ── MvpBlock (binding 7) ─────────────────────────────────────────────────

/// GPU mirror of `MvpBlock` — per-node model-view-projection matrix.
///
/// Used by geometry nodes (LineList, TriangleList, PointList).
/// Fullscreen nodes always upload the identity matrix — the field is present
/// in the bind group layout for all nodes so the layout stays unified.
///
/// Layout: column-major, matches WGSL `mat4x4<f32>`.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct MvpUniforms {
    /// Column-major 4×4 matrix (4 columns × 4 rows × 4 bytes = 64 bytes).
    pub matrix: [[f32; 4]; 4],
}

impl MvpUniforms {
    pub fn identity() -> Self {
        Self {
            matrix: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn from_columns(cols: [[f32; 4]; 4]) -> Self {
        Self { matrix: cols }
    }

    pub fn as_bytes(&self) -> &[u8] { bytemuck::bytes_of(self) }
}

/// Per-node uniform buffer for the MVP matrix.
///
/// One per node, created lazily, reused across frames.
/// Fullscreen nodes upload identity every frame (negligible cost — 64 bytes).
pub struct MvpUniformBuffer {
    pub buffer: wgpu::Buffer,
}

impl MvpUniformBuffer {
    pub fn new(device: &wgpu::Device, label: &str) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some(&format!("{label}_mvp")),
            size:               std::mem::size_of::<MvpUniforms>() as u64,
            usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Initialize with identity
        Self { buffer }
    }

    /// Upload the matrix from NodeConfig::mvp (or identity if None).
    pub fn update(&self, queue: &wgpu::Queue, mvp: Option<[[f32; 4]; 4]>) {
        let uniforms = match mvp {
            Some(m) => MvpUniforms::from_columns(m),
            None    => MvpUniforms::identity(),
        };
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
        assert_eq!(size_of::<FrameUniforms>(), 16);
    }

    #[test]
    fn custom_uniforms_size() {
        assert_eq!(size_of::<CustomUniforms>(), MAX_CUSTOM_UNIFORMS * 4);
    }

    #[test]
    fn mvp_uniforms_size_is_64_bytes() {
        // 4 columns × 4 rows × 4 bytes = 64 bytes. wgpu requires 16-byte alignment.
        assert_eq!(size_of::<MvpUniforms>(), 64);
    }

    #[test]
    fn mvp_identity_upload() {
        let m = MvpUniforms::identity();
        // Diagonal should be 1.0
        assert_eq!(m.matrix[0][0], 1.0);
        assert_eq!(m.matrix[1][1], 1.0);
        assert_eq!(m.matrix[2][2], 1.0);
        assert_eq!(m.matrix[3][3], 1.0);
        // Off-diagonal should be 0.0
        assert_eq!(m.matrix[0][1], 0.0);
        assert_eq!(m.matrix[1][0], 0.0);
    }

    #[test]
    fn frame_uniforms_from_ctx() {
        let ctx = FrameCtx { width: 1280, height: 720, time: 1.0, frame: 10, sample_count: 1 };
        let u = FrameUniforms::from_ctx(&ctx);
        assert_eq!(u.resolution, [1280.0, 720.0]);
        assert_eq!(u.time, 1.0);
        assert_eq!(u.frame, 10);
    }
}
