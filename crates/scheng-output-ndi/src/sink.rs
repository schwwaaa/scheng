//! `sink.rs` — NdiSink: OutputSink that broadcasts frames via NDI.

use scheng_graph::NodeId;
use scheng_runtime_wgpu::{executor::OutputSink, FrameCtx, RenderTarget};
use crate::{NdiConfig, NdiError};

#[cfg(feature = "ndi")]
use grafton_ndi::{NDI, Sender, SenderOptions, VideoFrame, PixelFormat, ScanType};

pub struct NdiSink {
    config:  NdiConfig,
    #[cfg(feature = "ndi")]
    sender:  Sender,
    #[cfg(not(feature = "ndi"))]
    _stub:   (),
}

impl NdiSink {
    pub fn new(config: NdiConfig) -> Result<Self, NdiError> {
        #[cfg(feature = "ndi")]
        {
            let ndi = NDI::new().map_err(|_| NdiError::SdkNotFound)?;

            let opts = SenderOptions::builder(&config.source_name)
                .groups(config.group.as_deref().unwrap_or("Public"))
                .clock_video(true)
                .build();

            let sender = Sender::new(&ndi, &opts)
                .map_err(|_| NdiError::CreateFailed { name: config.source_name.clone() })?;

            log::info!("NDI sender '{}' ready", config.source_name);
            return Ok(Self { config, sender });
        }

        #[cfg(not(feature = "ndi"))]
        {
            log::warn!(
                "[scheng-output-ndi] built without 'ndi' feature — NdiSink is a no-op. \
                 Add features = [\"ndi\"] and install NDI SDK to activate."
            );
            Ok(Self { config, _stub: () })
        }
    }

    pub fn source_name(&self) -> &str { &self.config.source_name }
}

impl OutputSink for NdiSink {
    fn present(
        &mut self,
        _node_id: NodeId,
        target:   &RenderTarget,
        ctx:      &FrameCtx,
        device:   &wgpu::Device,
        queue:    &wgpu::Queue,
    ) {
        #[cfg(feature = "ndi")]
        {
            let pixels = target.readback(device, queue);
            let stride = ctx.width * 4; // RGBA = 4 bytes per pixel

            let frame = VideoFrame {
                width:               ctx.width as i32,
                height:              ctx.height as i32,
                pixel_format:        PixelFormat::RGBA,
                scan_type:           ScanType::Progressive,
                frame_rate_n:        self.config.framerate_num as i32,
                frame_rate_d:        self.config.framerate_den as i32,
                line_stride_or_size: grafton_ndi::LineStrideOrSize::LineStrideBytes(stride as i32),
                data:                pixels,
                picture_aspect_ratio: 0.0,
                timecode:             i64::MIN, // NDI_SEND_TIMECODE_SYNTHESIZE
                metadata:             None,
                timestamp:            0,
            };

            self.sender.send_video(&frame);
        }

        #[cfg(not(feature = "ndi"))]
        let _ = (target, ctx, device, queue);
    }
}

impl Drop for NdiSink {
    fn drop(&mut self) {
        log::info!("NDI sender '{}' stopped", self.config.source_name);
    }
}
