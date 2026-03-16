//! FrameBlock GPU uniform buffer (uTime, uResolution, uFrame).

use bytemuck::{Pod, Zeroable};
use crate::FrameCtx;   // ← local, not scheng_core

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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn size_is_16_bytes() {
        assert_eq!(std::mem::size_of::<FrameUniforms>(), 16);
    }
    #[test]
    fn from_ctx_correct() {
        let ctx = FrameCtx { width: 1280, height: 720, time: 1.0, frame: 10 };
        let u = FrameUniforms::from_ctx(&ctx);
        assert_eq!(u.resolution, [1280.0, 720.0]);
        assert_eq!(u.time, 1.0);
        assert_eq!(u.frame, 10);
    }
}
