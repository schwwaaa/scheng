//! Per-frame execution context — defined locally, not from scheng-core.

/// Immutable per-frame context supplied by the host. The engine never owns time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameCtx {
    pub width:  u32,
    pub height: u32,
    pub time:   f32,
    pub frame:  u64,
}

impl Default for FrameCtx {
    fn default() -> Self {
        Self { width: 1280, height: 720, time: 0.0, frame: 0 }
    }
}
