//! `context.rs` — wgpu device and queue initialisation.
//!
//! `WgpuContext` is created once at startup and lives for the lifetime of the runtime.
//! All GPU resources (buffers, textures, pipelines) are created from `ctx.device`.
//!
//! Initialisation is synchronous from the caller's perspective: `WgpuContext::new()`
//! blocks internally using `pollster::block_on` so the render loop stays sync.

use crate::WgpuError;

/// Holds the wgpu adapter, device, and queue.
///
/// Created once; cloned into sub-systems (pipeline cache, render targets, etc.)
/// using `Arc` if needed — currently passed by reference throughout Phase 1.
pub struct WgpuContext {
    /// The wgpu logical device. Use this to create all GPU resources.
    pub device: wgpu::Device,
    /// The command queue. Use `queue.submit(...)` and `queue.write_buffer(...)`.
    pub queue: wgpu::Queue,
    /// Adapter info — useful for logging which GPU/backend is active.
    pub adapter_info: wgpu::AdapterInfo,
}

impl WgpuContext {
    /// Initialise a wgpu context with the best available backend.
    ///
    /// Backend priority (handled by wgpu automatically):
    /// - macOS → Metal
    /// - Windows → DX12, then Vulkan
    /// - Linux → Vulkan, then GL
    ///
    /// Blocks the calling thread until the device is ready.
    pub fn new() -> Result<Self, WgpuError> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Result<Self, WgpuError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            // WGPU_BACKEND env var can override this at runtime (e.g. for CI)
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        log::debug!("Requesting wgpu adapter (high performance, no surface)...");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                // No surface — we render offscreen only in Phase 1.
                // Phase 5 (Tauri) will pass a surface here for the preview window.
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or(WgpuError::NoAdapter)?;

        let adapter_info = adapter.get_info();
        log::info!(
            "wgpu adapter: {} ({:?}) — backend: {:?}",
            adapter_info.name,
            adapter_info.device_type,
            adapter_info.backend,
        );

        // Request features we need.
        // We keep the required set minimal so we work on as many GPUs as possible.
        let required_features = wgpu::Features::empty();

        // Raise the limits slightly for large textures / many bind groups.
        // `default()` gives us at least what spec guarantees — enough for Phase 1.
        let required_limits = wgpu::Limits::default();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("scheng-wgpu-device"),
                    required_features,
                    required_limits,
                    // No memory hints needed for Phase 1
                    memory_hints: wgpu::MemoryHints::default(),
                },
                // Trace path — None means no API call tracing
                None,
            )
            .await?;

        log::debug!("wgpu device created successfully");

        Ok(Self { device, queue, adapter_info })
    }
}
