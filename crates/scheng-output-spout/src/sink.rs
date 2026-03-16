//! `sink.rs` — SpoutSink for Windows.
//!
//! Implementation mirrors SyphonSink — same readback-based approach,
//! same OutputSink trait. The only difference is the native bridge.

#[cfg(target_os = "windows")]
mod windows_impl {
    use scheng_graph::NodeId;
    use scheng_runtime_wgpu::{executor::OutputSink, FrameCtx, RenderTarget};
    use crate::{ffi, SpoutError};

    pub struct SpoutSink {
        sender: *mut std::ffi::c_void,
        name:   String,
    }

    unsafe impl Send for SpoutSink {}
    unsafe impl Sync for SpoutSink {}

    impl SpoutSink {
        pub fn new(name: &str) -> Result<Self, SpoutError> {
            let c_name = std::ffi::CString::new(name)
                .unwrap_or_else(|_| std::ffi::CString::new("scheng").unwrap());
            let sender = unsafe { ffi::scheng_spout_create(c_name.as_ptr()) };
            if sender.is_null() {
                return Err(SpoutError::CreateFailed);
            }
            log::info!("Spout sender '{}' started", name);
            Ok(Self { sender, name: name.to_owned() })
        }

        pub fn name(&self) -> &str { &self.name }
    }

    impl Drop for SpoutSink {
        fn drop(&mut self) {
            if !self.sender.is_null() {
                unsafe { ffi::scheng_spout_destroy(self.sender) };
                self.sender = std::ptr::null_mut();
            }
        }
    }

    impl OutputSink for SpoutSink {
        fn present(
            &mut self,
            _node_id: NodeId,
            target:   &RenderTarget,
            _ctx:     &FrameCtx,
            device:   &wgpu::Device,
            queue:    &wgpu::Queue,
        ) {
            if self.sender.is_null() { return; }
            let pixels = target.readback(device, queue);
            unsafe {
                ffi::scheng_spout_send_rgba(
                    self.sender,
                    pixels.as_ptr(),
                    target.width,
                    target.height,
                );
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub use windows_impl::SpoutSink;
