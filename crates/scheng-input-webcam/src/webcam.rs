//! `webcam.rs` — Webcam capture with wgpu texture upload.

use crate::WebcamError;

// ── Stub (feature = "native" disabled) ───────────────────────────────────

#[cfg(not(feature = "native"))]
pub struct Webcam;

#[cfg(not(feature = "native"))]
impl Webcam {
    pub fn list_cameras() -> Vec<String> { vec![] }
    pub fn open(_index: u32, _width: u32, _height: u32,
                _device: &wgpu::Device, _queue: &wgpu::Queue)
        -> Result<Self, WebcamError>
    {
        Err(WebcamError::NotEnabled)
    }
    pub fn poll(&mut self, _queue: &wgpu::Queue) -> bool { false }
    pub fn texture_view(&self) -> Option<wgpu::TextureView> { None }
    pub fn texture_arc(&self) -> Option<std::sync::Arc<wgpu::Texture>> { None }
    pub fn width(&self)  -> u32 { 0 }
    pub fn height(&self) -> u32 { 0 }
}

// ── Real capture (feature = "native") ────────────────────────────────────

#[cfg(feature = "native")]
use nokhwa::{
    pixel_format::RgbAFormat,
    utils::{ApiBackend, CameraFormat, CameraIndex, FrameFormat,
            RequestedFormat, RequestedFormatType, Resolution},
    Camera,
};

#[cfg(feature = "native")]
pub struct Webcam {
    camera:  Camera,
    texture: std::sync::Arc<wgpu::Texture>,
    width:   u32,
    height:  u32,
}

#[cfg(feature = "native")]
impl Webcam {
    /// List all available cameras by name.
    pub fn list_cameras() -> Vec<String> {
        nokhwa::query(ApiBackend::Auto)
            .unwrap_or_default()
            .into_iter()
            .map(|info| format!("[{}] {}", info.index(), info.human_name()))
            .collect()
    }

    /// Open camera at `index`. Tries MJPEG first (most compatible on macOS),
    /// then YUYV, then lets the driver choose.
    pub fn open(index: u32, width: u32, height: u32,
                device: &wgpu::Device, queue: &wgpu::Queue)
        -> Result<Self, WebcamError>
    {
        // Log all supported formats so we know exactly what the camera accepts
        if let Ok(mut probe) = Camera::new(
            CameraIndex::Index(index),
            RequestedFormat::new::<RgbAFormat>(RequestedFormatType::AbsoluteHighestResolution),
        ) {
            for fmt in probe.compatible_camera_formats().unwrap_or_default() {
                log::info!("Webcam {index} supports: {:?}", fmt);
            }
        }

        // Try formats in order of macOS compatibility
        let candidates = [
            RequestedFormat::new::<RgbAFormat>(RequestedFormatType::Closest(
                CameraFormat::new_from(width, height, FrameFormat::MJPEG, 30)
            )),
            RequestedFormat::new::<RgbAFormat>(RequestedFormatType::Closest(
                CameraFormat::new_from(width, height, FrameFormat::YUYV, 30)
            )),
            RequestedFormat::new::<RgbAFormat>(RequestedFormatType::Closest(
                CameraFormat::new_from(640, 480, FrameFormat::MJPEG, 30)
            )),
        ];

        let mut camera = None;
        for fmt in &candidates {
            match Camera::new(CameraIndex::Index(index), *fmt) {
                Ok(c) => { camera = Some(c); break; }
                Err(_) => continue,
            }
        }
        let mut camera = camera
            .ok_or_else(|| WebcamError::OpenFailed("no compatible format found".into()))?;

        camera.open_stream()
            .map_err(|e| WebcamError::OpenFailed(e.to_string()))?;

        let cam_fmt  = camera.camera_format();
        let actual_w = cam_fmt.width();
        let actual_h = cam_fmt.height();

        let texture = Self::make_texture(device, actual_w, actual_h);
        let black = vec![0u8; (actual_w * actual_h * 4) as usize];
        Self::upload_pixels(queue, &texture, &black, actual_w, actual_h);

        log::info!("Webcam {index}: {}×{} opened ({:?})", actual_w, actual_h, cam_fmt.format());
        Ok(Self { camera, texture, width: actual_w, height: actual_h })
    }

    /// Poll for a new frame and upload to GPU. Non-blocking.
    pub fn poll(&mut self, queue: &wgpu::Queue) -> bool {
        match self.camera.frame() {
            Ok(raw) => match raw.decode_image::<RgbAFormat>() {
                Ok(img) => {
                    Self::upload_pixels(queue, &self.texture, img.as_raw(), self.width, self.height);
                    true
                }
                Err(e) => { log::warn!("Webcam decode: {e}"); false }
            },
            Err(_) => false,
        }
    }

    /// Texture view ready to bind as iChannel0.
    pub fn texture_view(&self) -> Option<wgpu::TextureView> {
        Some(self.texture.create_view(&wgpu::TextureViewDescriptor::default()))
    }

    /// Arc<Texture> for injection into NodeConfig::input_textures.
    pub fn texture_arc(&self) -> Option<std::sync::Arc<wgpu::Texture>> {
        Some(std::sync::Arc::clone(&self.texture))
    }

    pub fn width(&self)  -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }

    fn make_texture(device: &wgpu::Device, w: u32, h: u32) -> std::sync::Arc<wgpu::Texture> {
        std::sync::Arc::new(device.create_texture(&wgpu::TextureDescriptor {
            label:           Some("webcam_frame"),
            size:            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count:    1,
            dimension:       wgpu::TextureDimension::D2,
            format:          wgpu::TextureFormat::Rgba8Unorm,
            usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats:    &[],
        }))
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
            log::warn!("Webcam stop_stream: {e}");
        }
    }
}
