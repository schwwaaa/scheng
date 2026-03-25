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
    pub fn new() -> Result<Self, WgpuError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(), ..Default::default()
        });
        pollster::block_on(Self::init(instance, None))
    }

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

        // wgpu 23: request_device no longer takes a trace path argument.
        // trace moved into DeviceDescriptor.
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
