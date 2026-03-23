//! `receiver.rs` — RtmpReceiver: ffmpeg-based RTMP/RTSP live stream input.

use std::{
    io::Read,
    process::{Child, Command, Stdio},
    sync::{Arc, atomic::{AtomicBool, Ordering}},
    thread,
};
use crossbeam_channel::{bounded, Receiver, TrySendError};
use crate::RtmpError;

/// RTMP/RTSP stream receiver — pulls frames from ffmpeg and uploads to wgpu.
pub struct RtmpReceiver {
    frame_rx:  Receiver<Vec<u8>>,
    texture:   Option<Arc<wgpu::Texture>>,
    width:     u32,
    height:    u32,
    _process:  Child,
    _running:  Arc<AtomicBool>,
}

impl RtmpReceiver {
    /// Open a live stream URL. ffmpeg is spawned immediately and starts
    /// buffering frames. Call `poll()` each render frame to upload the latest.
    ///
    /// `width` and `height` are the expected output resolution.
    /// ffmpeg will scale the input to this size.
    pub fn open(
        url:    &str,
        width:  u32,
        height: u32,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
    ) -> Result<Self, RtmpError> {
        let args: Vec<String> = vec![
            // Suppress banner
            "-hide_banner".into(), "-loglevel".into(), "warning".into(),
            // Input stream
            "-i".into(), url.into(),
            // Scale to target resolution
            "-vf".into(), format!("scale={}:{}", width, height),
            // Output: raw RGBA to stdout
            "-f".into(), "rawvideo".into(),
            "-pix_fmt".into(), "rgba".into(),
            "-".into(), // stdout
        ];

        log::info!("[scheng-input-rtmp] Opening: {url} at {width}×{height}");

        let mut process = Command::new("ffmpeg")
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    RtmpError::FfmpegNotFound
                } else {
                    RtmpError::SpawnFailed(e.to_string())
                }
            })?;

        let mut stdout = process.stdout.take().expect("stdout missing");
        let frame_bytes = (width * height * 4) as usize;
        let (tx, rx) = bounded::<Vec<u8>>(4); // 4-frame buffer
        let running = Arc::new(AtomicBool::new(true));
        let running2 = Arc::clone(&running);

        thread::Builder::new()
            .name("scheng-rtmp-reader".into())
            .spawn(move || {
                let mut buf = vec![0u8; frame_bytes];
                loop {
                    if !running2.load(Ordering::Relaxed) { break; }

                    // Read exactly one frame
                    let mut total = 0;
                    while total < frame_bytes {
                        match stdout.read(&mut buf[total..]) {
                            Ok(0) => {
                                log::info!("[scheng-input-rtmp] Stream ended");
                                return;
                            }
                            Ok(n) => total += n,
                            Err(e) => {
                                log::error!("[scheng-input-rtmp] Read error: {e}");
                                return;
                            }
                        }
                    }

                    match tx.try_send(buf.clone()) {
                        Ok(()) => {}
                        Err(TrySendError::Full(_)) => {} // drop old frame
                        Err(TrySendError::Disconnected(_)) => break,
                    }
                }
            })
            .map_err(|e| RtmpError::SpawnFailed(e.to_string()))?;

        // Pre-allocate texture
        let texture = Arc::new(device.create_texture(&wgpu::TextureDescriptor {
            label:           Some("rtmp_input"),
            size:            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count:    1,
            dimension:       wgpu::TextureDimension::D2,
            format:          wgpu::TextureFormat::Rgba8Unorm,
            usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats:    &[],
        }));

        log::info!("[scheng-input-rtmp] Receiver started for '{url}'");

        Ok(Self {
            frame_rx:  rx,
            texture:   Some(texture),
            width,
            height,
            _process:  process,
            _running:  running,
        })
    }

    /// Poll for the latest frame and upload to the GPU texture.
    /// Non-blocking — returns false if no new frame is available.
    pub fn poll(&mut self, _device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        // Drain the channel, keep only the latest frame
        let mut latest: Option<Vec<u8>> = None;
        while let Ok(frame) = self.frame_rx.try_recv() {
            latest = Some(frame);
        }

        if let (Some(pixels), Some(tex)) = (latest, &self.texture) {
            queue.write_texture(
                tex.as_image_copy(),
                &pixels,
                wgpu::ImageDataLayout {
                    offset:         0,
                    bytes_per_row:  Some(self.width * 4),
                    rows_per_image: None,
                },
                wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
            );
            return true;
        }
        false
    }

    /// Wgpu texture view ready to bind as iChannel0. None until first frame.
    pub fn texture_view(&self) -> Option<wgpu::TextureView> {
        self.texture.as_ref().map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()))
    }

    /// Arc<Texture> for injection into NodeConfig::input_textures.
    pub fn texture_arc(&self) -> Option<Arc<wgpu::Texture>> {
        self.texture.as_ref().map(|t| Arc::clone(t))
    }

    pub fn width(&self)  -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }
}

impl Drop for RtmpReceiver {
    fn drop(&mut self) {
        self._running.store(false, Ordering::Relaxed);
        // Kill the ffmpeg process
        let _ = self._process.kill();
        log::info!("[scheng-input-rtmp] Receiver dropped");
    }
}
