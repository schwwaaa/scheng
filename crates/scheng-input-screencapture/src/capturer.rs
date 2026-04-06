//! `capturer.rs` — configurable screen capture → wgpu texture.
//!
//! Supports:
//! - Full-screen capture of any display by index
//! - Sub-region capture (x, y, width, height) in display pixels — like ScreenFlick
//! - Output rescaled to a fixed render resolution (independent of capture size)

use std::sync::Arc;
use screenshots::Screen;
use crate::ScreenCaptureError;

// ── CaptureConfig ─────────────────────────────────────────────────────────

/// Selects which display and which region to capture.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Which display to capture (0 = primary). Call `ScreenCapture::list_screens()`
    /// to enumerate available screens and their IDs.
    pub screen_index: usize,

    /// Optional sub-region in display pixels (x, y, width, height).
    /// `None` = full display.
    ///
    /// Like ScreenFlick's "Capture Area" selection — draw a rectangle over
    /// the part of the screen you want to feed into the shader graph.
    pub region: Option<(i32, i32, u32, u32)>,

    /// Output texture width uploaded to the GPU.
    /// The captured image is cropped/scaled to fit this.
    /// `None` = use the capture region's natural size.
    pub output_width: Option<u32>,

    /// Output texture height uploaded to the GPU.
    /// `None` = use the capture region's natural size.
    pub output_height: Option<u32>,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            screen_index:  0,
            region:        None,   // full display
            output_width:  Some(1280),
            output_height: Some(720),
        }
    }
}

impl CaptureConfig {
    /// Full primary display at native resolution.
    pub fn full_screen() -> Self {
        Self { screen_index: 0, region: None, output_width: None, output_height: None }
    }

    /// Capture a specific region of the primary display.
    /// Coordinates are in display pixels from the top-left corner.
    ///
    /// Example — capture the top-left quarter of a 2560×1440 display:
    /// ```rust,ignore
    /// CaptureConfig::region(0, 0, 1280, 720)
    /// ```
    pub fn region(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            screen_index:  0,
            region:        Some((x, y, width, height)),
            output_width:  Some(width),
            output_height: Some(height),
        }
    }

    /// Capture a region and rescale to the given output resolution.
    /// Useful when the capture area is large but you want a smaller GPU texture.
    pub fn region_scaled(x: i32, y: i32, width: u32, height: u32,
                         out_w: u32, out_h: u32) -> Self {
        Self {
            screen_index:  0,
            region:        Some((x, y, width, height)),
            output_width:  Some(out_w),
            output_height: Some(out_h),
        }
    }
}

// ── ScreenCapture ─────────────────────────────────────────────────────────

/// Captures a display (or region of one) each frame and uploads it as a
/// wgpu `Rgba8Unorm` texture, ready for injection into any node's iChannel.
pub struct ScreenCapture {
    screen:  Screen,
    config:  CaptureConfig,
    texture: Arc<wgpu::Texture>,
    width:   u32,
    height:  u32,
}

impl ScreenCapture {
    /// List all available screens. Returns (index, width, height, is_primary).
    pub fn list_screens() -> Result<Vec<(usize, u32, u32, bool)>, ScreenCaptureError> {
        let screens = Screen::all()
            .map_err(|e| ScreenCaptureError::InitFailed(e.to_string()))?;
        Ok(screens.iter().enumerate().map(|(i, s)| {
            let info = s.display_info;
            (i, info.width, info.height, info.is_primary)
        }).collect())
    }

    /// Create a screen capture source from a `CaptureConfig`.
    pub fn new(config: CaptureConfig, device: &wgpu::Device, queue: &wgpu::Queue)
        -> Result<Self, ScreenCaptureError>
    {
        let screens = Screen::all()
            .map_err(|e| ScreenCaptureError::InitFailed(e.to_string()))?;

        let screen = screens.into_iter().nth(config.screen_index)
            .ok_or_else(|| ScreenCaptureError::InitFailed(
                format!("screen index {} not found", config.screen_index)
            ))?;

        let info = screen.display_info;

        // Resolve capture dimensions
        let (cap_x, cap_y, cap_w, cap_h) = config.region
            .unwrap_or((0, 0, info.width, info.height));

        // Output texture dimensions
        let out_w = config.output_width.unwrap_or(cap_w);
        let out_h = config.output_height.unwrap_or(cap_h);

        let texture = Arc::new(make_texture(device, out_w, out_h));

        // Capture one frame at init so the texture is never uninitialised
        let raw = capture_region(&screen, cap_x, cap_y, cap_w, cap_h)?;
        let pixels = maybe_scale(&raw, cap_w, cap_h, out_w, out_h);
        upload(queue, &texture, &pixels, out_w, out_h);

        log::info!(
            "ScreenCapture: screen {} region ({},{}) {}×{} → output {}×{}",
            config.screen_index, cap_x, cap_y, cap_w, cap_h, out_w, out_h
        );

        Ok(Self { screen, config, texture, width: out_w, height: out_h })
    }

    /// Capture the current frame and upload. Returns true if a new frame arrived.
    pub fn poll(&mut self, _device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        let (cap_x, cap_y, cap_w, cap_h) = self.config.region
            .unwrap_or_else(|| {
                let info = self.screen.display_info;
                (0, 0, info.width, info.height)
            });

        match capture_region(&self.screen, cap_x, cap_y, cap_w, cap_h) {
            Ok(raw) => {
                let pixels = maybe_scale(&raw, cap_w, cap_h, self.width, self.height);
                upload(queue, &self.texture, &pixels, self.width, self.height);
                true
            }
            Err(e) => { log::warn!("ScreenCapture::poll: {e}"); false }
        }
    }

    /// `Arc<Texture>` for injection into `NodeConfig::input_textures[N]`.
    pub fn texture_arc(&self) -> Option<Arc<wgpu::Texture>> {
        Some(Arc::clone(&self.texture))
    }

    pub fn width(&self)  -> u32 { self.width  }
    pub fn height(&self) -> u32 { self.height }

    /// The active config (useful for displaying capture region in a UI).
    pub fn config(&self) -> &CaptureConfig { &self.config }
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn capture_region(screen: &Screen, x: i32, y: i32, w: u32, h: u32)
    -> Result<Vec<u8>, ScreenCaptureError>
{
    let image = screen.capture_area(x, y, w, h)
        .map_err(|e| ScreenCaptureError::CaptureFailed(e.to_string()))?;
    Ok(image.as_raw().to_vec())
}

/// Nearest-neighbour scale from (src_w×src_h) to (dst_w×dst_h).
/// If dimensions match, returns the original slice without copying.
fn maybe_scale(raw: &[u8], src_w: u32, src_h: u32,
               dst_w: u32, dst_h: u32) -> Vec<u8> {
    if src_w == dst_w && src_h == dst_h {
        return raw.to_vec();
    }
    let mut out = Vec::with_capacity((dst_w * dst_h * 4) as usize);
    for row in 0..dst_h {
        let src_row = (row as f32 / dst_h as f32 * src_h as f32) as u32;
        for col in 0..dst_w {
            let src_col = (col as f32 / dst_w as f32 * src_w as f32) as u32;
            let base = ((src_row * src_w + src_col) * 4) as usize;
            if base + 3 < raw.len() {
                out.extend_from_slice(&raw[base..base + 4]);
            } else {
                out.extend_from_slice(&[0, 0, 0, 255]);
            }
        }
    }
    out
}

fn make_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label:           Some("screencapture_frame"),
        size:            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count:    1,
        dimension:       wgpu::TextureDimension::D2,
        format:          wgpu::TextureFormat::Rgba8Unorm,
        usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
        view_formats:    &[],
    })
}

fn upload(queue: &wgpu::Queue, texture: &wgpu::Texture,
          pixels: &[u8], width: u32, height: u32) {
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture, mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row:  Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
}
