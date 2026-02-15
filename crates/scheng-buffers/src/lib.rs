use glow::HasContext;
use scheng_runtime_glow::{create_render_target, EngineError, RenderTarget};

/// A simple temporal ring buffer of GPU render targets.
///
/// B3 goal: make time-addressable buffers explicit and easy to emulate.
///
/// - Host decides when to push.
/// - Shaders can sample prior frames (bound as separate textures).
/// - This uses N separate 2D textures (not a texture array) for portability.
pub struct TemporalRing {
    w: i32,
    h: i32,
    slots: Vec<RenderTarget>,
    head: usize, // next write position
}

impl TemporalRing {
    pub fn new(gl: &glow::Context, w: i32, h: i32, capacity: usize) -> Result<Self, EngineError> {
        let cap = capacity.max(1);
        let mut slots = Vec::with_capacity(cap);
        for _ in 0..cap {
            slots.push(unsafe { create_render_target(gl, w, h)? });
        }
        Ok(Self {
            w,
            h,
            slots,
            head: 0,
        })
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }
    pub fn head(&self) -> usize {
        self.head
    }
    pub fn size(&self) -> (i32, i32) {
        (self.w, self.h)
    }

    /// Push the contents of `src_fbo` into the ring buffer by framebuffer blit.
    pub fn push_from_fbo(
        &mut self,
        gl: &glow::Context,
        src_fbo: glow::Framebuffer,
        src_w: i32,
        src_h: i32,
    ) {
        let dst = &self.slots[self.head];

        unsafe {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(src_fbo));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(dst.fbo));
            gl.blit_framebuffer(
                0,
                0,
                src_w,
                src_h,
                0,
                0,
                self.w,
                self.h,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
        }

        self.head = (self.head + 1) % self.slots.len();
    }

    /// Map "frames_ago" (0 = newest) to a slot index inside the ring.
    pub fn slot_for_frames_ago(&self, frames_ago: usize) -> usize {
        let n = self.slots.len();
        (self.head + n - 1 - (frames_ago % n)) % n
    }

    /// Convenience: get the texture handle for "frames_ago".
    pub fn tex_frames_ago(&self, frames_ago: usize) -> glow::Texture {
        let slot = self.slot_for_frames_ago(frames_ago);
        self.slots[slot].tex
    }
}
