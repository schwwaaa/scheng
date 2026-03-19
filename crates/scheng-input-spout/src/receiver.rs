//! `receiver.rs` — SpoutReceiver: connects to a Spout sender and uploads
//! frames to a wgpu RGBA texture each render cycle.

use crate::SpoutInputError;

// ── Non-Windows stub ──────────────────────────────────────────────────────

#[cfg(not(target_os = "windows"))]
pub struct SpoutReceiver;

#[cfg(not(target_os = "windows"))]
impl SpoutReceiver {
    pub fn list_senders() -> Vec<String> { vec![] }
    pub fn connect(_name: &str, _device: &wgpu::Device, _queue: &wgpu::Queue)
        -> Result<Self, SpoutInputError> { Err(SpoutInputError::NotWindows) }
    pub fn poll_with_device(&mut self, _device: &wgpu::Device, _queue: &wgpu::Queue) -> bool { false }
    pub fn texture_view(&self) -> Option<wgpu::TextureView> { None }
    pub fn width(&self)        -> u32 { 0 }
    pub fn height(&self)       -> u32 { 0 }
    pub fn is_connected(&self) -> bool { false }
}

// ── Windows implementation ────────────────────────────────────────────────

#[cfg(target_os = "windows")]
use crate::ffi;
#[cfg(target_os = "windows")]
use std::ffi::CString;

#[cfg(target_os = "windows")]
pub struct SpoutReceiver {
    handle:    *mut std::ffi::c_void,
    pixel_buf: Vec<u8>,
    width:     u32,
    height:    u32,
    texture:   Option<wgpu::Texture>,
}

#[cfg(target_os = "windows")]
unsafe impl Send for SpoutReceiver {}

#[cfg(target_os = "windows")]
impl SpoutReceiver {
    /// List available Spout senders on this machine.
    pub fn list_senders() -> Vec<String> {
        unsafe {
            // Allocate name buffers
            const MAX: u32 = 32;
            const BUF: u32 = 256;
            let mut bufs: Vec<Vec<u8>> = (0..MAX).map(|_| vec![0u8; BUF as usize]).collect();
            let mut ptrs: Vec<*mut std::ffi::c_char> =
                bufs.iter_mut().map(|b| b.as_mut_ptr() as *mut _).collect();

            let count = ffi::scheng_spout_receiver_list_senders(
                ptrs.as_mut_ptr(), MAX, BUF,
            );

            (0..count).map(|i| {
                std::ffi::CStr::from_ptr(ptrs[i as usize])
                    .to_string_lossy().into_owned()
            }).collect()
        }
    }

    /// Connect to a Spout sender by name.
    pub fn connect(
        name:   &str,
        device: &wgpu::Device,
        queue:  &wgpu::Queue,
    ) -> Result<Self, SpoutInputError> {
        unsafe {
            let handle = ffi::scheng_spout_receiver_create();
            if handle.is_null() {
                return Err(SpoutInputError::SdkNotWired);
            }

            let c_name = CString::new(name).unwrap();
            if ffi::scheng_spout_receiver_connect(handle, c_name.as_ptr()) == 0 {
                ffi::scheng_spout_receiver_destroy(handle);
                let available = Self::list_senders();
                return Err(SpoutInputError::SenderNotFound {
                    name: name.to_owned(), available,
                });
            }

            log::info!("SpoutReceiver: connected to '{}'", name);
            let _ = (device, queue);

            Ok(Self {
                handle,
                pixel_buf: Vec::new(),
                width:  0,
                height: 0,
                texture: None,
            })
        }
    }

    /// Poll for a new frame and upload to the wgpu texture.
    pub fn poll_with_device(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        let buf_size = (4096 * 4096 * 4) as usize;
        if self.pixel_buf.len() < buf_size {
            self.pixel_buf.resize(buf_size, 0);
        }

        let mut w: u32 = 0;
        let mut h: u32 = 0;

        let got = unsafe {
            ffi::scheng_spout_receiver_pull_rgba(
                self.handle,
                self.pixel_buf.as_mut_ptr(),
                &mut w, &mut h,
            )
        };

        if got == 0 || w == 0 || h == 0 { return false; }

        let pixels = &self.pixel_buf[..(w * h * 4) as usize];

        // Reallocate texture if dimensions changed
        if self.texture.is_none() || self.width != w || self.height != h {
            self.texture = Some(device.create_texture(&wgpu::TextureDescriptor {
                label:           Some("spout_input_frame"),
                size:            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count:    1,
                dimension:       wgpu::TextureDimension::D2,
                format:          wgpu::TextureFormat::Rgba8Unorm,
                usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats:    &[],
            }));
            self.width  = w;
            self.height = h;
            log::info!("SpoutReceiver: texture (re)allocated {}×{}", w, h);
        }

        if let Some(ref tex) = self.texture {
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: tex, mip_level: 0,
                    origin:  wgpu::Origin3d::ZERO,
                    aspect:  wgpu::TextureAspect::All,
                },
                pixels,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row:  Some(w * 4),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
        }
        true
    }

    pub fn texture_view(&self) -> Option<wgpu::TextureView> {
        self.texture.as_ref().map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()))
    }

    pub fn width(&self)        -> u32  { self.width }
    pub fn height(&self)       -> u32  { self.height }
    pub fn is_connected(&self) -> bool {
        unsafe { ffi::scheng_spout_receiver_is_connected(self.handle) != 0 }
    }
}

#[cfg(target_os = "windows")]
impl Drop for SpoutReceiver {
    fn drop(&mut self) {
        unsafe { ffi::scheng_spout_receiver_destroy(self.handle); }
        log::info!("SpoutReceiver: disconnected");
    }
}
