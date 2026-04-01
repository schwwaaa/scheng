# scheng Plugin Contract Specification

**Version:** 0.1.0  
**Status:** Stable  
**Lives in:** `crates/scheng-runtime-wgpu/src/plugin.rs`

---

## Overview

scheng has two plugin surfaces. Any Rust crate can become a scheng-compatible plugin by implementing one or both of these traits:

| Trait | Purpose | Naming convention |
|-------|---------|-------------------|
| `InputSource` | Produces GPU textures from an external source | `scheng-input-{protocol}` |
| `OutputSink` | Consumes rendered frames and delivers them | `scheng-output-{protocol}` |

Both traits are intentionally minimal. The SDK owns all GPU resource management — plugin authors implement only the protocol-specific logic.

---

## InputSource

```rust
pub trait InputSource {
    fn poll(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool;
    fn texture_arc(&self) -> Option<Arc<wgpu::Texture>>;
    fn width(&self)  -> u32;
    fn height(&self) -> u32;
    fn name(&self)   -> &str;

    // Optional overrides
    fn is_connected(&self)   -> bool              { self.texture_arc().is_some() }
    fn texture_format(&self) -> wgpu::TextureFormat { wgpu::TextureFormat::Rgba8Unorm }
}
```

### Method contracts

#### `poll(&mut self, device, queue) -> bool`

Called once per tick from the render thread, **before** `execute_frame()`.

- **Must not block.** If no new frame is ready, return `false` immediately.
- Return `true` if a new frame was uploaded to the GPU texture.
- Return `false` if no new data was available (previous frame remains valid).
- Upload new pixel data via `queue.write_texture()` to your existing texture.
- Do **not** reallocate the texture on every call — allocate once in the constructor.

#### `texture_arc(&self) -> Option<Arc<wgpu::Texture>>`

Returns the current GPU texture for injection into the render graph.

- Returns `None` only before the first frame has been received.
- After the first successful `poll()`, always return `Some`.
- The returned texture is assigned to `NodeConfig::input_textures[N]` and becomes `iChannelN` in GLSL.
- The texture must remain valid for the duration of the frame.

#### `width(&self) -> u32` / `height(&self) -> u32`

Return the pixel dimensions of the current texture.

- Must match the allocated texture dimensions.
- May change if the source changes resolution — reallocate the texture and update these values atomically.

#### `name(&self) -> &str`

Short, stable identifier for logging. Examples: `"ndi-receive"`, `"syphon-in"`, `"blackmagic-capture"`.

### Threading model

`InputSource` is **not** required to be `Send + Sync`. Implementations that run capture loops on background threads should manage threading internally and expose only the texture side via this trait.

The `poll()` method is always called from the GPU render thread. Use a channel or `Arc<Mutex<>>` to receive frames from background threads, then upload in `poll()`.

```rust
// Recommended pattern for background-thread captures
pub struct MyReceiver {
    rx:      std::sync::mpsc::Receiver<Vec<u8>>,
    texture: Arc<wgpu::Texture>,
    width:   u32,
    height:  u32,
}

impl InputSource for MyReceiver {
    fn poll(&mut self, _device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        match self.rx.try_recv() {
            Ok(pixels) => {
                queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture:   &*self.texture,
                        mip_level: 0,
                        origin:    wgpu::Origin3d::ZERO,
                        aspect:    wgpu::TextureAspect::All,
                    },
                    &pixels,
                    wgpu::ImageDataLayout {
                        offset:         0,
                        bytes_per_row:  Some(self.width * 4),
                        rows_per_image: None,
                    },
                    wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
                );
                true
            }
            Err(_) => false,
        }
    }
    // ...
}
```

### Texture format

Default format is `Rgba8Unorm`. If your source delivers a different format, override `texture_format()` and ensure the texture is created with the matching format.

The SDK does not convert formats between input sources and render targets. The compat layer handles `iChannelN` sampling generically.

### Minimal complete implementation

```rust
use std::sync::Arc;
use scheng_runtime_wgpu::plugin::InputSource;

pub struct MySource {
    texture: Arc<wgpu::Texture>,
    width:   u32,
    height:  u32,
}

impl MySource {
    pub fn open(width: u32, height: u32, device: &wgpu::Device, queue: &wgpu::Queue)
        -> Result<Self, Box<dyn std::error::Error>>
    {
        let texture = Arc::new(device.create_texture(&wgpu::TextureDescriptor {
            label:           Some("my-source"),
            size:            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count:    1,
            dimension:       wgpu::TextureDimension::D2,
            format:          wgpu::TextureFormat::Rgba8Unorm,
            usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats:    &[],
        }));

        // Upload black frame so texture is valid before first poll
        let black = vec![0u8; (width * height * 4) as usize];
        queue.write_texture(
            wgpu::ImageCopyTexture { texture: &*texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &black,
            wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(width * 4), rows_per_image: None },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );

        Ok(Self { texture, width, height })
    }
}

impl InputSource for MySource {
    fn poll(&mut self, _device: &wgpu::Device, _queue: &wgpu::Queue) -> bool {
        false // no-op until real implementation
    }
    fn texture_arc(&self) -> Option<Arc<wgpu::Texture>> { Some(Arc::clone(&self.texture)) }
    fn width(&self)  -> u32 { self.width  }
    fn height(&self) -> u32 { self.height }
    fn name(&self)   -> &str { "my-source" }
}
```

---

## OutputSink

```rust
pub trait OutputSink {
    fn present(
        &mut self,
        node_id: NodeId,
        target:  &RenderTarget,
        ctx:     &FrameCtx,
        device:  &wgpu::Device,
        queue:   &wgpu::Queue,
    );

    // Optional overrides
    fn name(&self)       -> &str { "output-sink" }
    fn shutdown(&mut self) {}
}
```

### Method contracts

#### `present(&mut self, node_id, target, ctx, device, queue)`

Called once per `PixelsOut` node per frame, **after** `queue.submit()`.

The render target texture has been fully written by the GPU before this method is called.

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `node_id` | `NodeId` | The `PixelsOut` node that triggered this sink |
| `target` | `&RenderTarget` | The completed frame — see fields below |
| `ctx` | `&FrameCtx` | Frame time, index, and resolution |
| `device` | `&wgpu::Device` | For lightweight GPU ops only |
| `queue` | `&wgpu::Queue` | For lightweight GPU ops only |

**RenderTarget fields available to sinks:**

```rust
pub struct RenderTarget {
    pub texture:     wgpu::Texture,      // Rgba16Float — the rendered frame
    pub render_view: wgpu::TextureView,  // MSAA render view (may be multisampled)
    pub sample_view: wgpu::TextureView,  // Resolved view — use this for reading
    pub width:       u32,
    pub height:      u32,
}
```

**Always use `target.sample_view`** for reading pixels or sharing textures. `render_view` may be a multisampled texture when MSAA is active.

**Do not** record new render passes from inside `present()`. The command encoder has been submitted. Use `queue.write_buffer()` or `queue.write_texture()` for lightweight updates only.

#### `shutdown(&mut self)`

Called when the instrument exits cleanly. Override to flush buffers, finalize recordings, or close connections. Default does nothing.

### Common patterns

#### Pixel readback (for encoders, file output, network streaming)

```rust
fn present(&mut self, _id: NodeId, target: &RenderTarget, ctx: &FrameCtx,
           device: &wgpu::Device, queue: &wgpu::Queue) {
    let w = target.width as usize;
    let h = target.height as usize;
    let bytes_per_row = (w * 4 + 255) & !255; // 256-byte alignment

    // Create staging buffer
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label:              Some("readback"),
        size:               (bytes_per_row * h) as u64,
        usage:              wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture:   &target.texture,
            mip_level: 0,
            origin:    wgpu::Origin3d::ZERO,
            aspect:    wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &staging,
            layout: wgpu::ImageDataLayout {
                offset:         0,
                bytes_per_row:  Some(bytes_per_row as u32),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d { width: target.width, height: target.height, depth_or_array_layers: 1 },
    );
    queue.submit(std::iter::once(enc.finish()));

    // Map and read (blocking — run on dedicated output thread for production)
    let slice = staging.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::Maintain::Wait);
    let pixels: Vec<u8> = slice.get_mapped_range().to_vec();

    // Send to encoder, network, etc.
    self.send_frame(pixels, target.width, target.height, ctx.time);
}
```

#### Zero-copy texture sharing (Syphon pattern)

```rust
fn present(&mut self, _id: NodeId, target: &RenderTarget, _ctx: &FrameCtx,
           device: &wgpu::Device, _queue: &wgpu::Queue) {
    // Share target.texture (or the Metal texture behind it) directly
    // No CPU readback — the GPU texture is passed to the receiver
    self.share_metal_texture(&target.texture, target.width, target.height);
}
```

### Minimal complete implementation

```rust
use scheng_runtime_wgpu::plugin::OutputSink;
use scheng_runtime_wgpu::{RenderTarget, FrameCtx};
use scheng_graph::NodeId;

pub struct MyOutput {
    frames_presented: u64,
}

impl OutputSink for MyOutput {
    fn present(&mut self, _id: NodeId, target: &RenderTarget,
               ctx: &FrameCtx, _device: &wgpu::Device, _queue: &wgpu::Queue) {
        self.frames_presented += 1;
        log::debug!("[MyOutput] frame {} — {}×{} t={:.2}s",
            self.frames_presented, target.width, target.height, ctx.time);
    }
    fn name(&self) -> &str { "my-output" }
    fn shutdown(&mut self) {
        log::info!("[MyOutput] shutdown after {} frames", self.frames_presented);
    }
}
```

---

## Cargo.toml template for plugin crates

```toml
[package]
name        = "scheng-input-myprotocol"
version     = "0.1.0"
edition     = "2021"
description = "scheng input plugin — MyProtocol capture"
keywords    = ["scheng", "video", "plugin", "myprotocol"]
categories  = ["multimedia::video"]

[dependencies]
scheng-runtime-wgpu = "0.1"      # semver — patch updates are always compatible
scheng-graph        = "0.1"
wgpu                = "23"
log                 = "0.4"

# Your protocol-specific dependencies
# my-capture-sdk = "2.1"
```

---

## Checklist for plugin authors

Before publishing to crates.io:

- [ ] Implements `InputSource` or `OutputSink` (or both)
- [ ] `poll()` never blocks — uses `try_recv()` or equivalent
- [ ] Texture allocated once in constructor, not reallocated per frame
- [ ] `name()` returns a short, stable identifier
- [ ] `shutdown()` flushes any buffers or closes connections
- [ ] Tested with `cargo test` (headless GPU tests encouraged)
- [ ] Declares supported scheng version range in `Cargo.toml`
- [ ] `README.md` includes a minimal usage example
- [ ] Platform-specific code gated with `#[cfg(target_os)]`
- [ ] No `unwrap()` in production paths — return `Result` or log and degrade gracefully

---

## PluginInfo (optional convention)

Plugin crates may optionally expose a `PluginInfo` constant for runtime discovery:

```rust
use scheng_runtime_wgpu::plugin::PluginInfo;

pub const INFO: PluginInfo = PluginInfo {
    name:            "scheng-input-blackmagic",
    version:         env!("CARGO_PKG_VERSION"),
    description:     "Blackmagic Design DeckLink capture input for scheng",
    min_sdk_version: "0.1.0",
    platforms:       &["macos", "windows"],
};
```

This is not required by the SDK. It is a convention for plugin registries or instrument authors who want to inspect plugin metadata at runtime.

---

## Versioning policy

`InputSource` and `OutputSink` traits follow semantic versioning.

| Change type | Version bump |
|-------------|-------------|
| Add optional method with default impl | Patch |
| Add required method | **Major** |
| Remove or rename method | **Major** |
| Change method signature | **Major** |

Plugin authors should pin to `scheng-runtime-wgpu = "0.1"` (minor-compatible) and check the changelog on any major version bump.

---

## SBC / Embedded targets

scheng targets are Raspberry Pi 4/5 and NVIDIA Jetson on the embedded roadmap. Plugin authors targeting these platforms:

- Avoid GPU readback in hot paths — GPU→CPU copies are expensive on shared-memory architectures
- Prefer zero-copy texture sharing where the platform supports it
- Jetson: wgpu Vulkan backend works, NDI SDK has an ARM build
- Raspberry Pi: wgpu Vulkan via Mesa (Pi 5 recommended — Pi 4 Vulkan support is limited)
- Gate platform-specific code with `#[cfg(target_arch = "aarch64")]` as appropriate
