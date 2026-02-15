#![allow(clippy::missing_safety_doc)]

use glow::HasContext;
use scheng_runtime_glow::{create_render_target, EngineError, RenderTarget};

/// A simple ping-pong render target pair for internal feedback.
///
/// Semantics:
/// - `prev()` is the texture you sample from (previous frame)
/// - `next()` is the FBO/texture you render into (current frame)
/// - after rendering, call `swap()`
pub struct PingPongTarget {
    a: RenderTarget,
    b: RenderTarget,
    a_is_prev: bool,
    width: i32,
    height: i32,
}

impl PingPongTarget {
    pub unsafe fn new(gl: &glow::Context, width: i32, height: i32) -> Result<Self, EngineError> {
        let a = create_render_target(gl, width, height)?;
        let b = create_render_target(gl, width, height)?;

        // Initialize to black (avoid undefined sampling on first frame)
        for rt in [&a, &b] {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(rt.fbo));
            gl.viewport(0, 0, width, height);
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);

        Ok(Self {
            a,
            b,
            a_is_prev: true,
            width,
            height,
        })
    }

    pub fn size(&self) -> (i32, i32) {
        (self.width, self.height)
    }

    /// The texture handle you should sample as feedback.
    pub fn prev_tex(&self) -> glow::NativeTexture {
        if self.a_is_prev {
            self.a.tex
        } else {
            self.b.tex
        }
    }

    /// The render target you should draw into for this frame.
    pub fn next_target(&self) -> &RenderTarget {
        if self.a_is_prev {
            &self.b
        } else {
            &self.a
        }
    }

    /// Swap prev/next after completing a frame render.
    pub fn swap(&mut self) {
        self.a_is_prev = !self.a_is_prev;
    }

    /// Recreate both targets at a new size (cleared to black).
    pub unsafe fn resize(
        &mut self,
        gl: &glow::Context,
        width: i32,
        height: i32,
    ) -> Result<(), EngineError> {
        // destroy old
        gl.delete_texture(self.a.tex);
        gl.delete_framebuffer(self.a.fbo);
        gl.delete_texture(self.b.tex);
        gl.delete_framebuffer(self.b.fbo);

        self.a = create_render_target(gl, width, height)?;
        self.b = create_render_target(gl, width, height)?;
        self.width = width;
        self.height = height;
        self.a_is_prev = true;

        for rt in [&self.a, &self.b] {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(rt.fbo));
            gl.viewport(0, 0, width, height);
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);

        Ok(())
    }
}
