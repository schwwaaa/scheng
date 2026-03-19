use std::ffi::CString;
use scheng_graph::NodeId;
use scheng_runtime_wgpu::{executor::OutputSink, FrameCtx, RenderTarget};
use crate::{ffi, SyphonError};

pub struct SyphonSink {
    server: *mut std::ffi::c_void,
    name:   String,
}

unsafe impl Send for SyphonSink {}
unsafe impl Sync for SyphonSink {}

impl SyphonSink {
    pub fn new(name: &str) -> Result<Self, SyphonError> {
        let c_name = CString::new(name).unwrap_or_else(|_| CString::new("scheng").unwrap());
        let server = unsafe { ffi::scheng_syphon_create(c_name.as_ptr()) };
        if server.is_null() {
            return Err(SyphonError::CreateFailed);
        }
        log::info!("Syphon server '{}' started", name);
        Ok(Self { server, name: name.to_owned() })
    }

    pub fn name(&self) -> &str { &self.name }
}

impl Drop for SyphonSink {
    fn drop(&mut self) {
        if !self.server.is_null() {
            unsafe { ffi::scheng_syphon_destroy(self.server) };
            self.server = std::ptr::null_mut();
        }
    }
}

impl OutputSink for SyphonSink {
    fn present(&mut self, _node_id: NodeId, target: &RenderTarget,
               _ctx: &FrameCtx, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.server.is_null() { return; }

        let pixels = target.readback(device, queue);

        unsafe {
            ffi::scheng_syphon_publish_rgba(
                self.server,
                pixels.as_ptr(),
                target.width,
                target.height,
            );
        }
    }
}
