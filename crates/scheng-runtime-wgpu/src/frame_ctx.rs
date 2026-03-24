//! `frame_ctx.rs` — per-frame execution context.
//!
//! Defined locally in scheng-runtime-wgpu so the crate has no dependency
//! on scheng-runtime-glow. The field layout matches the glow crate's FrameCtx
//! exactly — they are structurally identical and can be trivially converted.

/// The immutable per-frame execution context supplied by the host.
///
/// The engine never generates or mutates this — the instrument owns time.
/// Pass a new `FrameCtx` to `WgpuRuntime::execute_frame` each frame.
///
/// # Field semantics
///
/// - `width` / `height` — render resolution in pixels. Changing these between
///   frames triggers render target reallocation (cheap but not free).
/// - `time` — seconds since start. Not required to be monotonic. Supports
///   scrubbing, looping, and discontinuous jumps.
/// - `frame` — monotonic frame counter. Used for `uFrame` in shaders.
///   The engine does not enforce increment rules.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameCtx {
    /// Render width in pixels.
    pub width: u32,
    /// Render height in pixels.
    pub height: u32,
    /// Seconds since instrument start (passed to `uTime` uniform).
    pub time: f32,
    /// Monotonic frame counter (passed to `uFrame` uniform).
    pub frame: u64,
    /// MSAA sample count. 1 = off, 4 = 4x (default for --msaa 4).
    /// Changing between frames triggers render target reallocation.
    pub sample_count: u32,
}

impl Default for FrameCtx {
    fn default() -> Self {
        Self { width: 1280, height: 720, time: 0.0, frame: 0, sample_count: 1 }
    }
}
