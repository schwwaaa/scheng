//! wgpu Device + Queue initialisation.

use crate::WgpuError;

pub struct WgpuContext {
    pub device:       wgpu::Device,
    pub queue:        wgpu::Queue,
    pub adapter_info: wgpu::AdapterInfo,
    pub instance:     wgpu::Instance,
    pub adapter:      wgpu::Adapter,
}

impl WgpuContext {
    /// Headless init — no window surface required.
    pub fn new() -> Result<Self, WgpuError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(), ..Default::default()
        });
        pollster::block_on(Self::init(instance, None))
    }

    /// Surface-compatible init.
    ///
    /// The caller must create the instance AND surface first, then pass both in.
    /// This guarantees the adapter is chosen from the same instance the surface
    /// belongs to — mixing instances causes wgpu to panic.
    ///
    /// ```rust,ignore
    /// let instance = wgpu::Instance::new(...);
    /// let surface  = instance.create_surface(&window)?;
    /// let ctx      = WgpuContext::new_with_surface(instance, &surface)?;
    /// ```
    pub fn new_with_surface(
        instance: wgpu::Instance,
        surface:  &wgpu::Surface,
    ) -> Result<Self, WgpuError> {
        pollster::block_on(Self::init(instance, Some(surface)))
    }

    async fn init(
        instance: wgpu::Instance,
        surface:  Option<&wgpu::Surface<'_>>,
    ) -> Result<Self, WgpuError> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference:       wgpu::PowerPreference::HighPerformance,
                compatible_surface:     surface,
                force_fallback_adapter: false,
            })
            .await
            .ok_or(WgpuError::NoAdapter)?;

        let adapter_info = adapter.get_info();
        log::info!(
            "wgpu adapter: {} ({:?}) backend: {:?}",
            adapter_info.name, adapter_info.device_type, adapter_info.backend
        );

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label:             Some("scheng-wgpu-device"),
                    required_features: wgpu::Features::empty(),
                    required_limits:   wgpu::Limits::default(),
                    memory_hints:      wgpu::MemoryHints::default(),
                },
                None,
            )
            .await?;

        Ok(Self { device, queue, adapter_info, instance, adapter })
    }
}
