//! `sink.rs` — NdiSink: OutputSink for NDI.
//!
//! # NDI SDK integration
//!
//! This stub defines the complete interface. To activate, wire in your
//! preferred NDI Rust crate where the TODO comments are.
//!
//! Good options:
//! - github.com/hansjorg/ndi-rs (raw bindings, matches this API closely)
//! - github.com/fhttimer/ndi-sdk-rs
//! - Direct libndi FFI (C headers from the NDI SDK)
//!
//! # Threading note
//!
//! NDI send operations are typically fast (~1ms per frame at 1080p30).
//! For higher resolutions or tight timing, consider wrapping NdiSink in
//! a dedicated thread with a bounded channel (same pattern as FfmpegSink).

use scheng_graph::NodeId;
use scheng_runtime_wgpu::{executor::OutputSink, FrameCtx, RenderTarget};

use crate::{NdiConfig, NdiError};

/// OutputSink that sends frames via NDI.
///
/// Discoverable by OBS NDI input, Resolume, vMix, and any NDI receiver
/// on the local network.
pub struct NdiSink {
    config: NdiConfig,
    // TODO: replace with actual NDI sender handle
    // sender: ndi::SendInstance,
    active: bool,
}

impl NdiSink {
    /// Create an NDI sender with the given config.
    pub fn new(config: NdiConfig) -> Result<Self, NdiError> {
        // TODO: initialise NDI SDK and create sender
        // Example with ndi-rs:
        //
        // ndi::initialize().map_err(|_| NdiError::SdkNotFound)?;
        // let sender = ndi::SendInstance::builder(&config.source_name)
        //     .groups(&config.group)
        //     .build()
        //     .map_err(|_| NdiError::CreateFailed { name: config.source_name.clone() })?;
        //
        // For now, log a clear message and activate as a no-op sink.

        log::warn!(
            "[scheng-output-ndi] NDI SDK not yet wired up — NdiSink is a stub. \
             See src/sink.rs TODO comments to activate."
        );
        log::info!("NDI sink '{}' (stub) created", config.source_name);

        Ok(Self { config, active: false })
    }

    pub fn source_name(&self) -> &str { &self.config.source_name }
    pub fn is_active(&self) -> bool   { self.active }
}

impl OutputSink for NdiSink {
    fn present(
        &mut self,
        _node_id: NodeId,
        target:   &RenderTarget,
        ctx:      &FrameCtx,
        device:   &wgpu::Device,
        queue:    &wgpu::Queue,
    ) {
        if !self.active {
            // Stub: no-op until NDI SDK is wired in.
            return;
        }

        let pixels = target.readback(device, queue);

        // TODO: send frame via NDI SDK
        // Example with ndi-rs:
        //
        // let frame = ndi::VideoFrame::builder()
        //     .width(ctx.width as i32)
        //     .height(ctx.height as i32)
        //     .fourcc(ndi::FourCCVideoType::RGBA)
        //     .frame_rate(self.config.framerate_num as i32,
        //                 self.config.framerate_den as i32)
        //     .data(&pixels)
        //     .build();
        //
        // self.sender.send_video(&frame);

        log::trace!(
            "NDI stub: frame {}×{} frame={} (not sent — SDK not wired)",
            ctx.width, ctx.height, ctx.frame
        );
        let _ = pixels; // suppress unused warning until SDK is wired
    }
}

impl Drop for NdiSink {
    fn drop(&mut self) {
        // TODO: release NDI sender and uninitialise SDK if this is the last sender
        log::info!("NDI sink '{}' stopped", self.config.source_name);
    }
}
