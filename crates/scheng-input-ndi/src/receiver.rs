//! `receiver.rs` — NDI source receiver → wgpu RGBA texture.

use std::time::Duration;
use crate::NdiError;

#[cfg(feature = "ndi")]
use grafton_ndi::{
    NDI, Finder, FinderOptions, Receiver, ReceiverOptions,
    ReceiverBandwidth,
};

/// NDI source descriptor — name and URL as discovered on the network.
#[derive(Debug, Clone)]
pub struct NdiSource {
    pub name: String,
    pub url:  String,
}

/// NDI receiver — connects to a source and uploads frames to a wgpu texture.
pub struct NdiReceiver {
    source_name: String,
    width:       u32,
    height:      u32,
    texture:     Option<wgpu::Texture>,
    #[cfg(feature = "ndi")]
    receiver:    Receiver,
}

impl NdiReceiver {
    /// Discover NDI sources on the local network.
    /// Blocks for `timeout_ms` milliseconds while listening for announcements.
    pub fn find_sources(timeout_ms: u32) -> Result<Vec<NdiSource>, NdiError> {
        #[cfg(feature = "ndi")]
        {
            let ndi = NDI::new().map_err(|_| NdiError::SdkNotFound)?;
            let opts = FinderOptions::builder()
                .show_local_sources(true)
                .build();
            let finder = Finder::new(&ndi, &opts)
                .map_err(|_| NdiError::SdkNotFound)?;
            let sources = finder
                .find_sources(Duration::from_millis(timeout_ms as u64))
                .map_err(|e| NdiError::ReceiveError(e.to_string()))?;
            let result = sources.iter().map(|s| NdiSource {
                name: s.name.clone(),
                url:  s.ip_address().unwrap_or_default().to_string(),
            }).collect();
            log::info!("[scheng-input-ndi] found {} source(s)", sources.len());
            return Ok(result);
        }

        #[cfg(not(feature = "ndi"))]
        {
            log::warn!("[scheng-input-ndi] built without 'ndi' feature — no sources available");
            Ok(vec![])
        }
    }

    /// Open a named NDI source for receiving.
    pub fn open(
        source:  &NdiSource,
        _device: &wgpu::Device,
        _queue:  &wgpu::Device,
    ) -> Result<Self, NdiError> {
        #[cfg(feature = "ndi")]
        {
            let ndi = NDI::new().map_err(|_| NdiError::SdkNotFound)?;

            let opts = FinderOptions::builder()
                .show_local_sources(true)
                .build();
            let finder = Finder::new(&ndi, &opts)
                .map_err(|_| NdiError::SdkNotFound)?;
            let sources = finder
                .find_sources(Duration::from_millis(500))
                .map_err(|e| NdiError::ReceiveError(e.to_string()))?;

            let ndi_source = sources
                .iter()
                .find(|s| s.name == source.name)
                .ok_or_else(|| NdiError::SourceNotFound {
                    name:       source.name.clone(),
                    timeout_ms: 500,
                })?;

            let recv_opts = ReceiverOptions::builder(ndi_source.clone())
                .bandwidth(ReceiverBandwidth::Highest)
                .allow_video_fields(false)
                .build();

            let receiver = Receiver::new(&ndi, &recv_opts)
                .map_err(|_| NdiError::SourceNotFound {
                    name:       source.name.clone(),
                    timeout_ms: 500,
                })?;

            log::info!("[scheng-input-ndi] connected to '{}'", source.name);
            return Ok(Self {
                source_name: source.name.clone(),
                width:    0,
                height:   0,
                texture:  None,
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

    /// Poll for a new NDI frame and upload to the wgpu texture.
    /// Non-blocking — returns false if no frame is available.
    pub fn poll(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        #[cfg(feature = "ndi")]
        {
            match self.receiver.capture_video(Duration::ZERO) {
                Ok(frame) => {
                    let w = frame.width as u32;
                    let h = frame.height as u32;
                    if w > 0 && h > 0 {
                        self.upload(device, queue, w, h, &frame.data);
                        return true;
                    }
                }
                Err(_) => {}
            }
            return false;
        }

        #[cfg(not(feature = "ndi"))]
        false
    }

    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue:  &wgpu::Queue,
        w:      u32,
        h:      u32,
        data:   &[u8],
    ) {
        if self.texture.is_none() || self.width != w || self.height != h {
            self.texture = Some(device.create_texture(&wgpu::TextureDescriptor {
                label:           Some("ndi_input"),
                size:            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count:    1,
                dimension:       wgpu::TextureDimension::D2,
                format:          wgpu::TextureFormat::Rgba8Unorm,
                usage:           wgpu::TextureUsages::TEXTURE_BINDING
                                 | wgpu::TextureUsages::COPY_DST,
                view_formats:    &[],
            }));
            self.width  = w;
            self.height = h;
            log::info!("[scheng-input-ndi] '{}' frame: {}×{}", self.source_name, w, h);
        }

        if let Some(tex) = &self.texture {
            queue.write_texture(
                tex.as_image_copy(),
                data,
                wgpu::ImageDataLayout {
                    offset:         0,
                    bytes_per_row:  Some(w * 4),
                    rows_per_image: None,
                },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
        }
    }

    /// Wgpu texture view ready to bind as iChannel0. None until first frame.
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
