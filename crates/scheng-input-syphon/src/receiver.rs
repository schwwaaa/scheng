//! `receiver.rs` — SyphonReceiver: connects to a Syphon server and uploads
//! frames to a wgpu RGBA texture each render cycle.

use std::ffi::CString;

use crate::SyphonInputError;

/// Information about an available Syphon server.
#[derive(Debug, Clone)]
pub struct SyphonServerInfo {
    /// Server name (e.g. "Composition" in Resolume, "OBS" in OBS).
    pub name: String,
    /// Publishing application name.
    pub app:  String,
}

// ── Non-macOS stub ────────────────────────────────────────────────────────

#[cfg(not(target_os = "macos"))]
pub struct SyphonReceiver;

#[cfg(not(target_os = "macos"))]
impl SyphonReceiver {
    pub fn list_servers(_mtl_device: *mut std::ffi::c_void) -> Vec<SyphonServerInfo> { vec![] }
    pub fn connect(_name: &str, _mtl_device: *mut std::ffi::c_void,
                   _device: &wgpu::Device, _queue: &wgpu::Queue)
        -> Result<Self, SyphonInputError> { Err(SyphonInputError::NotMacOs) }
    pub fn poll(&mut self, _queue: &wgpu::Queue) -> bool { false }
    pub fn poll_with_device(&mut self, _device: &wgpu::Device, _queue: &wgpu::Queue) -> bool { false }
    pub fn texture_view(&self) -> Option<wgpu::TextureView> { None }
    pub fn width(&self)        -> u32 { 0 }
    pub fn height(&self)       -> u32 { 0 }
    pub fn is_connected(&self) -> bool { false }
}

// ── macOS stub when framework is not enabled ──────────────────────────────

#[cfg(all(target_os = "macos", not(feature = "syphon-framework")))]
pub struct SyphonReceiver;

#[cfg(all(target_os = "macos", not(feature = "syphon-framework")))]
impl SyphonReceiver {
    pub fn list_servers(_mtl_device: *mut std::ffi::c_void) -> Vec<SyphonServerInfo> {
        log::warn!("scheng-input-syphon: built without syphon-framework feature — no real Syphon support");
        vec![]
    }
    pub fn connect(_name: &str, _mtl_device: *mut std::ffi::c_void,
                   _device: &wgpu::Device, _queue: &wgpu::Queue)
        -> Result<Self, SyphonInputError>
    {
        Err(SyphonInputError::ConnectFailed {
            name: "syphon-framework feature not enabled — rebuild with --features syphon-framework".into()
        })
    }
    pub fn poll(&mut self, _queue: &wgpu::Queue) -> bool { false }
    pub fn poll_with_device(&mut self, _device: &wgpu::Device, _queue: &wgpu::Queue) -> bool { false }
    pub fn texture_view(&self) -> Option<wgpu::TextureView> { None }
    pub fn width(&self)        -> u32 { 0 }
    pub fn height(&self)       -> u32 { 0 }
    pub fn is_connected(&self) -> bool { false }
}

// ── macOS full implementation (requires syphon-framework feature) ─────────

#[cfg(all(target_os = "macos", feature = "syphon-framework"))]
use crate::ffi;

#[cfg(all(target_os = "macos", feature = "syphon-framework"))]
pub struct SyphonReceiver {
    directory:  *mut std::ffi::c_void,
    client:     *mut std::ffi::c_void,
    mtl_device: *mut std::ffi::c_void,
    /// Staging pixel buffer — resized on frame dimension change.
    pixel_buf:  Vec<u8>,
    /// Current frame dimensions (updated on each successful pull).
    width:      u32,
    height:     u32,
    /// wgpu texture — reallocated when dimensions change.
    texture:    Option<wgpu::Texture>,
}

#[cfg(all(target_os = "macos", feature = "syphon-framework"))]
// SAFETY: SyphonReceiver is used on the render thread only.
// MTLDevice and Syphon objects are thread-safe for our usage pattern
// (single-threaded poll + upload).
unsafe impl Send for SyphonReceiver {}

#[cfg(all(target_os = "macos", feature = "syphon-framework"))]
impl SyphonReceiver {
    /// List all currently available Syphon servers.
    ///
    /// `mtl_device` — the MTLDevice pointer from your wgpu Metal HAL.
    pub fn list_servers(_mtl_device: *mut std::ffi::c_void) -> Vec<SyphonServerInfo> {
        unsafe {
            let dir   = ffi::scheng_syphon_directory_create();
            let count = ffi::scheng_syphon_server_count(dir);
            let mut out = Vec::new();
            for i in 0..count {
                let name_ptr = ffi::scheng_syphon_server_name(dir, i);
                let app_ptr  = ffi::scheng_syphon_server_app(dir, i);
                let name = if name_ptr.is_null() { String::new() }
                           else { std::ffi::CStr::from_ptr(name_ptr).to_string_lossy().into_owned() };
                let app  = if app_ptr.is_null()  { String::new() }
                           else { std::ffi::CStr::from_ptr(app_ptr).to_string_lossy().into_owned() };
                out.push(SyphonServerInfo { name, app });
            }
            ffi::scheng_syphon_directory_destroy(dir);
            out
        }
    }

    /// Connect to a Syphon server by name.
    ///
    /// `mtl_device` — raw `id<MTLDevice>` pointer from wgpu Metal HAL:
    /// ```rust,ignore
    /// use wgpu::hal::api::Metal;
    /// let mut ptr = std::ptr::null_mut();
    /// device.as_hal::<Metal, _, _>(|d| {
    ///     if let Some(d) = d { ptr = d.raw_device().as_ptr() as *mut _; }
    /// });
    /// ```
    pub fn connect(
        name:       &str,
        mtl_device: *mut std::ffi::c_void,
        device:     &wgpu::Device,
        queue:      &wgpu::Queue,
    ) -> Result<Self, SyphonInputError> {
        unsafe {
            let dir = ffi::scheng_syphon_directory_create();

            let c_name  = CString::new(name).unwrap();
            let client  = ffi::scheng_syphon_client_create(dir, c_name.as_ptr(), mtl_device);

            if client.is_null() {
                // Collect available server names for the error message
                let count = ffi::scheng_syphon_server_count(dir);
                let available: Vec<String> = (0..count).map(|i| {
                    let p = ffi::scheng_syphon_server_name(dir, i);
                    if p.is_null() { String::new() }
                    else { std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned() }
                }).collect();
                ffi::scheng_syphon_directory_destroy(dir);
                return Err(SyphonInputError::ServerNotFound {
                    name: name.to_owned(), available,
                });
            }

            log::info!("SyphonReceiver: connected to '{}'", name);
            let _ = (device, queue); // reserved for initial texture allocation

            Ok(Self {
                directory: dir,
                client,
                mtl_device,
                pixel_buf: Vec::new(),
                width:     0,
                height:    0,
                texture:   None,
            })
        }
    }

    /// Poll for a new Syphon frame. If one is available, upload to the wgpu texture.
    /// Returns `true` if a new frame was received.
    pub fn poll(&mut self, queue: &wgpu::Queue) -> bool {
        // Allocate a generous staging buffer — resize on dimension change
        let buf_size = (4096 * 4096 * 4) as usize; // max ~64MB, safe on unified memory
        if self.pixel_buf.len() < buf_size {
            self.pixel_buf.resize(buf_size, 0);
        }

        let mut w: u32 = 0;
        let mut h: u32 = 0;

        let got_frame = unsafe {
            ffi::scheng_syphon_client_pull_rgba(
                self.client,
                self.pixel_buf.as_mut_ptr(),
                &mut w,
                &mut h,
                self.mtl_device,
            )
        };

        if got_frame == 0 || w == 0 || h == 0 {
            return false;
        }

        let pixel_bytes = (w * h * 4) as usize;
        let pixels      = &self.pixel_buf[..pixel_bytes];

        // Upload to wgpu texture — reallocate if dimensions changed
        if let Some(ref tex) = self.texture {
            if self.width == w && self.height == h {
                // Same size — just write
                queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: tex, mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    pixels,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row:  Some(w * 4),
                        rows_per_image: Some(h),
                    },
                    wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                );
                return true;
            }
        }

        // Dimension changed or first frame — drop the old texture, we'll create
        // a new one on next poll when we have device access.
        // For now, log and mark dimensions — full texture creation needs &device.
        // The caller must call poll_with_device() if they need reallocation.
        self.width  = w;
        self.height = h;
        log::debug!("SyphonReceiver: frame {}×{} received", w, h);
        true
    }

    /// Poll with device access — required when frame dimensions may change.
    /// Use this variant in the render loop for correct behaviour.
    pub fn poll_with_device(
        &mut self,
        device: &wgpu::Device,
        queue:  &wgpu::Queue,
    ) -> bool {
        let buf_size = (4096 * 4096 * 4) as usize;
        if self.pixel_buf.len() < buf_size {
            self.pixel_buf.resize(buf_size, 0);
        }

        let mut w: u32 = 0;
        let mut h: u32 = 0;

        let got_frame = unsafe {
            ffi::scheng_syphon_client_pull_rgba(
                self.client,
                self.pixel_buf.as_mut_ptr(),
                &mut w,
                &mut h,
                self.mtl_device,
            )
        };

        if got_frame == 0 || w == 0 || h == 0 {
            return false;
        }

        let pixel_bytes = (w * h * 4) as usize;
        let pixels      = &self.pixel_buf[..pixel_bytes];

        // Reallocate texture if dimensions changed
        if self.texture.is_none() || self.width != w || self.height != h {
            self.texture = Some(device.create_texture(&wgpu::TextureDescriptor {
                label:           Some("syphon_input_frame"),
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
            log::info!("SyphonReceiver: texture (re)allocated {}×{}", w, h);
        }

        // Upload
        if let Some(ref tex) = self.texture {
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: tex, mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
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

    /// A wgpu texture view of the latest Syphon frame, ready to bind as iChannel0.
    pub fn texture_view(&self) -> Option<wgpu::TextureView> {
        self.texture.as_ref().map(|t| {
            t.create_view(&wgpu::TextureViewDescriptor::default())
        })
    }

    pub fn width(&self)        -> u32  { self.width }
    pub fn height(&self)       -> u32  { self.height }
    pub fn is_connected(&self) -> bool {
        unsafe { ffi::scheng_syphon_client_is_connected(self.client) != 0 }
    }
}

#[cfg(all(target_os = "macos", feature = "syphon-framework"))]
impl Drop for SyphonReceiver {
    fn drop(&mut self) {
        unsafe {
            ffi::scheng_syphon_client_destroy(self.client);
            ffi::scheng_syphon_directory_destroy(self.directory);
        }
        log::info!("SyphonReceiver: disconnected");
    }
}
