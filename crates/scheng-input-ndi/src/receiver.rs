//! `receiver.rs` — NDI source receiver → wgpu RGBA texture.

use crate::NdiError;

#[cfg(feature = "ndi")]
use {
    std::sync::Arc,
    std::time::Duration,
    grafton_ndi::{NDI, Finder, FinderOptions, Receiver, ReceiverOptions,
                  RecvColorFormat, RecvBandwidth, VideoFrame},
};

/// NDI source descriptor returned by `find_sources`.
#[derive(Debug, Clone)]
pub struct NdiSource {
    pub name: String,
    pub url:  String,
}

/// NDI receiver — polls for frames and uploads to a wgpu texture.
pub struct NdiReceiver {
    source_name: String,
    width:       u32,
    height:      u32,
    texture:     Option<wgpu::Texture>,
    #[cfg(feature = "ndi")]
    _ndi:        Arc<NDI>,
    #[cfg(feature = "ndi")]
    receiver:    Receiver,
}

impl NdiReceiver {
    /// Discover NDI sources on the local network.
    /// `timeout_ms`: how long to wait (2000ms is typical for discovery).
    pub fn find_sources(timeout_ms: u32) -> Result<Vec<NdiSource>, NdiError> {
        #[cfg(feature = "ndi")]
        {
            let ndi = NDI::new().map_err(|_| NdiError::SdkNotFound)?;
            let opts = FinderOptions::builder().show_local_sources(true).build();
            let finder = Finder::new(&ndi, &opts).map_err(|_| NdiError::SdkNotFound)?;
            let sources = finder
                .find_sources(Duration::from_millis(timeout_ms as u64))
                .map_err(|e| NdiError::ReceiveError(e.to_string()))?;
            return Ok(sources.iter().map(|s| NdiSource {
                name: s.name().to_string(),
                url:  s.url_address().unwrap_or_default().to_string(),
            }).collect());
        }

        #[cfg(not(feature = "ndi"))]
        {
            log::warn!("[scheng-input-ndi] built without 'ndi' feature — returning empty source list");
            Ok(vec![])
        }
    }

    /// Open a specific NDI source for receiving.
    pub fn open(
        source: &NdiSource,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
    ) -> Result<Self, NdiError> {
        #[cfg(feature = "ndi")]
        {
            let ndi = Arc::new(NDI::new().map_err(|_| NdiError::SdkNotFound)?);
            let opts = ReceiverOptions::builder()
                .source_name(&source.name)
                .color_format(RecvColorFormat::RGBA_BGRA)
                .bandwidth(RecvBandwidth::Highest)
                .allow_video_fields(false)
                .build();
            let receiver = Receiver::new(&ndi, &opts)
                .map_err(|_| NdiError::SourceNotFound { name: source.name.clone() })?;
            log::info!("NDI receiver opened: '{}'", source.name);
            return Ok(Self {
                source_name: source.name.clone(),
                width:   0,
                height:  0,
                texture: None,
                _ndi:    ndi,
                receiver,
            });
        }

        #[cfg(not(feature = "ndi"))]
        {
            log::warn!("[scheng-input-ndi] stub — source '{}' not connected", source.name);
            Ok(Self {
                source_name: source.name.clone(),
                width:   0,
                height:  0,
                texture: None,
            })
        }
    }

    /// Poll for a new frame and upload to GPU. Returns true if a new frame arrived.
    pub fn poll(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        #[cfg(feature = "ndi")]
        {
            match self.receiver.capture_video(0) {
                Ok(Some(frame)) => {
                    self.upload_frame(device, queue, &frame);
                    return true;
                }
                _ => return false,
            }
        }
        #[cfg(not(feature = "ndi"))]
        false
    }

    #[cfg(feature = "ndi")]
    fn upload_frame(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &VideoFrame) {
        let w = frame.width() as u32;
        let h = frame.height() as u32;

        if self.texture.is_none() || self.width != w || self.height != h {
            self.texture = Some(device.create_texture(&wgpu::TextureDescriptor {
                label:           Some("ndi_receive"),
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
            log::info!("NDI '{}' frame size: {}×{}", self.source_name, w, h);
        }

        if let Some(tex) = &self.texture {
            queue.write_texture(
                tex.as_image_copy(),
                frame.data(),
                wgpu::TexelCopyBufferLayout {
                    offset:         0,
                    bytes_per_row:  Some(w * 4),
                    rows_per_image: None,
                },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
        }
    }

    /// Wgpu texture view ready to bind as an input texture. None until first frame.
    pub fn texture_view(&self) -> Option<wgpu::TextureView> {
        self.texture.as_ref().map(|t| {
            t.create_view(&wgpu::TextureViewDescriptor::default())
        })
    }

    pub fn source_name(&self) -> &str { &self.source_name }
    pub fn width(&self)       -> u32  { self.width }
    pub fn height(&self)      -> u32  { self.height }
    pub fn is_active(&self)   -> bool { self.texture.is_some() }
}
