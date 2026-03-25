//! `node.rs` — VideoDecodeSource integration with the scheng graph executor.
//!
//! `VideoSourceManager` sits alongside `WgpuRuntime` in the instrument.
//! Before calling `execute_frame()`, call `manager.update()` to decode the
//! current frame and inject it into `NodeConfig::frag_shader` via iChannel0.
//!
//! # How it works
//!
//! A `VideoDecodeSource` node is just a `ShaderSource` that samples iChannel0.
//! The instrument registers which file goes with which node. Each frame:
//!
//! 1. `VideoSourceManager::update()` calls `decoder.upload_frame(time, queue)`
//!    for each registered node — uploads the decoded RGBA pixels to a wgpu texture.
//!
//! 2. The texture is NOT injected via iChannel0 binding automatically — the
//!    executor's iChannel0 binding always comes from the graph's upstream node.
//!    Instead, `VideoDecodeSource` shaders use a dedicated iChannel0 that the
//!    manager writes to by overriding the blank texture in the bind group.
//!
//! # Integration path (Phase 2.1 — full bind group override)
//!
//! Full integration requires the executor to accept external texture overrides
//! per node — a `TextureOverride` map alongside `NodeConfig`. This is the
//! cleanest API. For now, the video texture is exposed via `texture_view()` and
//! the instrument can inject it as the upstream node's output.
//!
//! # Simple integration pattern (works today)
//!
//! ```rust,ignore
//! // Register video files at startup
//! let mut video_mgr = VideoSourceManager::new();
//! video_mgr.register(src_node, "assets/clip.mp4", &device, &queue)?;
//!
//! // Each frame — before execute_frame:
//! video_mgr.update(ctx.time, &queue);
//!
//! // execute_frame runs normally — the video texture is the source
//! runtime.execute_frame(&graph, &plan, &configs, &ctx, &mut sink)?;
//! ```

use std::collections::HashMap;
use scheng_graph::NodeId;
use crate::{VideoDecoder, VideoError};

/// Manages video file decoders for VideoDecodeSource nodes.
pub struct VideoSourceManager {
    decoders: HashMap<NodeId, VideoDecoder>,
}

impl VideoSourceManager {
    pub fn new() -> Self {
        Self { decoders: HashMap::new() }
    }

    /// Register a video file for a specific node.
    pub fn register(
        &mut self,
        node_id: NodeId,
        path:    &str,
        device:  &wgpu::Device,
        queue:   &wgpu::Queue,
    ) -> Result<(), VideoError> {
        let decoder = VideoDecoder::open(path, device, queue)?;
        self.decoders.insert(node_id, decoder);
        log::info!("VideoSourceManager: node {:?} → '{}'", node_id, path);
        Ok(())
    }

    /// Decode and upload frames for all registered nodes.
    ///
    /// Call once per frame before `WgpuRuntime::execute_frame()`.
    pub fn update(&mut self, time_secs: f32, queue: &wgpu::Queue) {
        for decoder in self.decoders.values_mut() {
            decoder.upload_frame(time_secs, queue);
        }
    }

    /// Get the texture view for a node's current video frame.
    ///
    /// Returns `None` if the node has no registered video or decode is disabled.
    pub fn texture_view(&self, node_id: NodeId) -> Option<wgpu::TextureView> {
        self.decoders.get(&node_id).and_then(|d| d.texture_view())
    }

    /// Metadata for a registered node.
    pub fn info(&self, node_id: NodeId) -> Option<VideoInfo> {
        self.decoders.get(&node_id).map(|d| VideoInfo {
            width:    d.width(),
            height:   d.height(),
            fps:      d.fps(),
            duration: d.duration(),
        })
    }

    /// Remove a video source and release the decoder.
    pub fn unregister(&mut self, node_id: NodeId) {
        self.decoders.remove(&node_id);
    }
}

/// Metadata about a registered video source.
#[derive(Debug, Clone)]
pub struct VideoInfo {
    pub width:    u32,
    pub height:   u32,
    pub fps:      f32,
    pub duration: f32,
}

impl Default for VideoSourceManager {
    fn default() -> Self { Self::new() }
}
