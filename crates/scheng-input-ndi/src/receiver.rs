//! `receiver.rs` — NDI source receiver with wgpu texture upload.

use crate::NdiError;

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
    // TODO: add NDI receiver handle here once SDK is wired:
    // receiver: ndi::RecvInstance,
    // texture:  wgpu::Texture,
    active: bool,
}

impl NdiReceiver {
    /// Discover NDI sources on the local network.
    ///
    /// `timeout_ms`: how long to wait for discovery (2000ms is typical).
    pub fn find_sources(_timeout_ms: u32) -> Result<Vec<NdiSource>, NdiError> {
        // TODO: wire NDI SDK discovery
        // Example with ndi-rs:
        //   ndi::initialize()?;
        //   let finder = ndi::FindInstance::builder().build()?;
        //   std::thread::sleep(Duration::from_millis(timeout_ms as u64));
        //   let sources = finder.get_sources(timeout_ms)?;
        //   Ok(sources.iter().map(|s| NdiSource { name: s.name(), url: s.url() }).collect())
        log::warn!("[scheng-input-ndi] NDI SDK not wired — find_sources returning empty list");
        Ok(vec![])
    }

    /// Open an NDI source by name for receiving.
    pub fn open(source: &NdiSource, _device: &wgpu::Device, _queue: &wgpu::Queue)
        -> Result<Self, NdiError>
    {
        // TODO: wire NDI SDK receiver creation
        // Example:
        //   let recv = ndi::RecvInstance::builder(&source.name, "", ndi::RecvColorFormat::RGBA)
        //       .build()
        //       .map_err(|e| NdiError::ReceiveError(e.to_string()))?;
        log::warn!("[scheng-input-ndi] NDI receiver is a stub — source '{}' not actually connected",
            source.name);

        Ok(Self {
            source_name: source.name.clone(),
            width:  0,
            height: 0,
            active: false,
        })
    }

    /// Poll for a new NDI frame and upload to GPU if available.
    /// Returns true if a new frame arrived.
    pub fn poll(&mut self, _queue: &wgpu::Queue) -> bool {
        // TODO: wire NDI frame receive + texture upload
        // Example:
        //   match self.recv.capture_video(0) {
        //       Some(frame) => {
        //           self.texture.upload(queue, frame.data());
        //           true
        //       }
        //       None => false,
        //   }
        false
    }

    /// A wgpu texture view ready to bind as iChannel0. None until first frame.
    pub fn texture_view(&self) -> Option<wgpu::TextureView> {
        None // TODO: return real texture view once SDK is wired
    }

    pub fn source_name(&self) -> &str { &self.source_name }
    pub fn width(&self)       -> u32  { self.width }
    pub fn height(&self)      -> u32  { self.height }
    pub fn is_active(&self)   -> bool { self.active }
}
