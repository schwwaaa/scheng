//! `webcam.rs` — Webcam capture with wgpu texture upload.

use crate::WebcamError;

// ── Stub (feature = "native" disabled) ───────────────────────────────────

#[cfg(not(feature = "native"))]
pub struct Webcam;

#[cfg(not(feature = "native"))]
impl Webcam {
    pub fn open(_index: u32, _width: u32, _height: u32,
                _device: &wgpu::Device, _queue: &wgpu::Queue)
        -> Result<Self, WebcamError>
    {
        Err(WebcamError::NotEnabled)
    }
    pub fn poll(&mut self, _queue: &wgpu::Queue) -> bool { false }
    pub fn texture_view(&self) -> Option<wgpu::TextureView> { None }
    pub fn width(&self)  -> u32 { 0 }
    pub fn height(&self) -> u32 { 0 }
}

// ── Real capture (feature = "native") ────────────────────────────────────

#[cfg(feature = "native")]
use nokhwa::{
    pixel_format::RgbAFormat,
    utils::{CameraIndex, RequestedFormat, RequestedFormatType, Resolution},
    Camera,
};

#[cfg(feature = "native")]
pub struct Webcam {
    camera:    Camera,
    texture:   wgpu::Texture,
    width:     u32,
    height:    u32,
    new_frame: bool,
}

#[cfg(feature = "native")]
impl Webcam {
    /// Open the camera at `index` (0 = first/default camera).
    pub fn open(index: u32, width: u32, height: u32,
                device: &wgpu::Device, queue: &wgpu::Queue)
        -> Result<Self, WebcamError>
    {
        let fmt = RequestedFormat::new::<RgbAFormat>(
            RequestedFormatType::Closest(Resolution::new(width, height))
        );
        let mut camera = Camera::new(CameraIndex::Index(index), fmt)
            .map_err(|e| WebcamError::OpenFailed(e.to_string()))?;

        camera.open_stream()
            .map_err(|e| WebcamError::OpenFailed(e.to_string()))?;

        // Get actual resolution after negotiation
        let fmt      = camera.camera_format();
        let actual_w = fmt.width();
        let actual_h = fmt.height();

        let texture = Self::make_texture(device, actual_w, actual_h);

        // Upload a black frame so the texture is valid before the first poll
        let black = vec![0u8; (actual_w * actual_h * 4) as usize];
        Self::upload_pixels(queue, &texture, &black, actual_w, actual_h);

        log::info!("Webcam {}: {}×{} opened", index, actual_w, actual_h);

        Ok(Self { camera, texture, width: actual_w, height: actual_h, new_frame: false })
    }

    /// Poll for a new frame and upload if available. Returns true if a new frame arrived.
    ///
    /// Non-blocking — if the camera has no new frame ready, returns immediately.
    pub fn poll(&mut self, queue: &wgpu::Queue) -> bool {
        match self.camera.frame() {
            Ok(raw) => {
                match raw.decode_image::<RgbAFormat>() {
                    Ok(img) => {
                        Self::upload_pixels(
                            queue, &self.texture,
                            img.as_raw(), self.width, self.height
                        );
                        self.new_frame = true;
                        true
                    }
                    Err(e) => {
                        log::warn!("Webcam decode failed: {e}");
                        false
                    }
                }
            }
            Err(_) => false, // no new frame ready
        }
    }

    /// A wgpu texture view ready to bind as iChannel0.
    pub fn texture_view(&self) -> Option<wgpu::TextureView> {
        Some(self.texture.create_view(&wgpu::TextureViewDescriptor::default()))
    }

    pub fn width(&self)  -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }

    fn make_texture(device: &wgpu::Device, w: u32, h: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label:           Some("webcam_frame"),
            size:            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count:    1,
            dimension:       wgpu::TextureDimension::D2,
            format:          wgpu::TextureFormat::Rgba8Unorm,
            usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats:    &[],
        })
    }

    fn upload_pixels(queue: &wgpu::Queue, texture: &wgpu::Texture,
                     pixels: &[u8], w: u32, h: u32) {
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture, mip_level: 0,
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
}

#[cfg(feature = "native")]
impl Drop for Webcam {
    fn drop(&mut self) {
        if let Err(e) = self.camera.stop_stream() {
            log::warn!("Webcam stop_stream failed: {e}");
        }
    }
}
