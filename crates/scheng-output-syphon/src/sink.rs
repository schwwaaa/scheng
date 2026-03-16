//! `sink.rs` — SyphonSink: implements OutputSink for Syphon on macOS.

use std::ffi::CString;

use scheng_graph::NodeId;
use scheng_runtime_wgpu::{executor::OutputSink, FrameCtx, RenderTarget};

use crate::{ffi, SyphonError};

/// Publishes rendered frames to a Syphon server.
///
/// Visible to OBS (Syphon input plugin), Resolume, VDMX, TouchDesigner,
/// and any other Syphon-capable app on the same Mac.
///
/// # Frame path
///
/// ```text
/// WgpuRuntime renders frame → wgpu RenderTarget (RGBA8 Metal texture)
///   → target.readback() → Vec<u8> RGBA pixels
///   → scheng_syphon_publish_rgba() → SyphonMetalServer
///   → Syphon clients (OBS, Resolume, etc.)
/// ```
///
/// On Apple Silicon (M1/M2), the readback → upload path uses unified
/// memory — no true copy occurs, just pointer aliasing.
///
/// # MTLDevice pointer
///
/// The MTLDevice raw pointer must come from wgpu's Metal HAL:
///
/// ```rust,no_run
/// #[cfg(target_os = "macos")]
/// unsafe {
///     use wgpu::hal::api::Metal;
///     runtime.ctx.device.as_hal::<Metal, _, _>(|hal_device| {
///         if let Some(d) = hal_device {
///             let mtl_device_ptr = d.raw_device().as_ptr() as *mut std::ffi::c_void;
///             let sink = SyphonSink::new("scheng", mtl_device_ptr).unwrap();
///         }
///     });
/// }
/// ```
pub struct SyphonSink {
    server:     *mut std::ffi::c_void,
    mtl_device: *mut std::ffi::c_void,
    name:       String,
}

// SAFETY: SyphonMetalServer is thread-safe for publish operations.
unsafe impl Send for SyphonSink {}
unsafe impl Sync for SyphonSink {}

impl SyphonSink {
    /// Create a Syphon server with the given name.
    ///
    /// `mtl_device` must be a valid `id<MTLDevice>` pointer obtained from
    /// wgpu's Metal HAL (see struct-level docs for the exact call).
    pub fn new(name: &str, mtl_device: *mut std::ffi::c_void) -> Result<Self, SyphonError> {
        if mtl_device.is_null() {
            return Err(SyphonError::CreateFailed);
        }

        let c_name = CString::new(name).unwrap_or_else(|_| CString::new("scheng").unwrap());

        let server = unsafe {
            ffi::scheng_syphon_create(c_name.as_ptr(), mtl_device)
        };

        if server.is_null() {
            return Err(SyphonError::CreateFailed);
        }

        log::info!("Syphon server '{}' started", name);
        Ok(Self { server, mtl_device, name: name.to_owned() })
    }

    /// Returns true if any Syphon clients are currently connected.
    ///
    /// Use this to skip readback when no one is watching — saves GPU→CPU bandwidth.
    pub fn has_clients(&self) -> bool {
        unsafe { ffi::scheng_syphon_has_clients(self.server) != 0 }
    }

    pub fn name(&self) -> &str { &self.name }
}

impl Drop for SyphonSink {
    fn drop(&mut self) {
        if !self.server.is_null() {
            unsafe { ffi::scheng_syphon_destroy(self.server) };
            self.server = std::ptr::null_mut();
            log::info!("Syphon server '{}' stopped", self.name);
        }
    }
}

impl OutputSink for SyphonSink {
    fn present(
        &mut self,
        _node_id: NodeId,
        target:   &RenderTarget,
        _ctx:     &FrameCtx,
        device:   &wgpu::Device,
        queue:    &wgpu::Queue,
    ) {
        if self.server.is_null() { return; }

        // Optional optimisation: skip readback if no clients are watching.
        // Comment out if you need the server always active for client discovery.
        if !self.has_clients() {
            return;
        }

        // Readback rendered RGBA pixels.
        // GPU work is already submitted (executor calls present() after queue.submit()).
        let pixels = target.readback(device, queue);

        unsafe {
            ffi::scheng_syphon_publish_rgba(
                self.server,
                pixels.as_ptr(),
                target.width,
                target.height,
                self.mtl_device,
            );
        }
    }
}
