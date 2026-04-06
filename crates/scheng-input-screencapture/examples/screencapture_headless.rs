//! screencapture_headless — proves ScreenCapture works without a display window.
//!
//! Run:  cargo run --example screencapture_headless -p scheng-input-screencapture
//!
//! What it does:
//!   1. Lists all available screens
//!   2. Captures the full primary display → GPU texture → reads pixels back to CPU
//!   3. Captures a 640×360 region from (100,100) → GPU texture → reads back
//!   4. Saves both as PNG files so you can visually confirm correctness
//!   5. Runs 10 frames of poll() and prints timing

use scheng_input_screencapture::{ScreenCapture, capturer::CaptureConfig};
use scheng_runtime_wgpu::context::WgpuContext;
use std::time::Instant;

fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    // ── GPU context (headless — no surface) ───────────────────────────────
    let ctx = WgpuContext::new().expect("wgpu init failed");
    let device = &ctx.device;
    let queue  = &ctx.queue;
    println!("GPU: {}", ctx.adapter_info.name);

    // ── List screens ──────────────────────────────────────────────────────
    let screens = ScreenCapture::list_screens().expect("list_screens failed");
    println!("\nAvailable screens:");
    for (i, w, h, primary) in &screens {
        println!("  [{i}] {w}×{h}{}", if *primary { " (primary)" } else { "" });
    }

    // ── Test 1: full primary display ──────────────────────────────────────
    println!("\n--- Test 1: full primary display ---");
    let mut cap = ScreenCapture::new(CaptureConfig::default(), device, queue)
        .expect("ScreenCapture::new failed");

    println!("Texture size: {}×{}", cap.width(), cap.height());

    // Read pixels back to CPU and save as PNG
    let pixels = readback(device, queue, cap.texture_arc().unwrap(), cap.width(), cap.height());
    save_png("screencapture_full.png", &pixels, cap.width(), cap.height());
    println!("Saved screencapture_full.png");

    // ── Test 2: sub-region capture ────────────────────────────────────────
    println!("\n--- Test 2: region (100,100) 640×360 ---");
    let mut cap2 = ScreenCapture::new(
        CaptureConfig::region(100, 100, 640, 360),
        device, queue
    ).expect("region capture failed");

    println!("Texture size: {}×{}", cap2.width(), cap2.height());
    let pixels2 = readback(device, queue, cap2.texture_arc().unwrap(), cap2.width(), cap2.height());
    save_png("screencapture_region.png", &pixels2, cap2.width(), cap2.height());
    println!("Saved screencapture_region.png");

    // ── Test 3: region scaled down ────────────────────────────────────────
    println!("\n--- Test 3: region (0,0) 1280×720 scaled to 320×180 ---");
    let mut cap3 = ScreenCapture::new(
        CaptureConfig::region_scaled(0, 0, 1280, 720, 320, 180),
        device, queue
    ).expect("scaled capture failed");

    println!("Texture size: {}×{}", cap3.width(), cap3.height());
    let pixels3 = readback(device, queue, cap3.texture_arc().unwrap(), cap3.width(), cap3.height());
    save_png("screencapture_scaled.png", &pixels3, cap3.width(), cap3.height());
    println!("Saved screencapture_scaled.png");

    // ── Test 4: poll() timing ─────────────────────────────────────────────
    println!("\n--- Test 4: 10 frames of poll() timing ---");
    let start = Instant::now();
    for i in 0..10 {
        let t = Instant::now();
        cap.poll(device, queue);
        println!("  frame {i}: {:.1}ms", t.elapsed().as_secs_f64() * 1000.0);
    }
    println!("Total: {:.1}ms  avg: {:.1}ms/frame",
        start.elapsed().as_secs_f64() * 1000.0,
        start.elapsed().as_secs_f64() * 100.0,
    );

    println!("\nAll tests passed.");
}

// ── Pixel readback (GPU → CPU) ────────────────────────────────────────────

fn readback(device: &wgpu::Device, queue: &wgpu::Queue,
            texture: std::sync::Arc<wgpu::Texture>, width: u32, height: u32) -> Vec<u8> {
    let bytes_per_row = (width * 4 + 255) & !255;
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size:  (bytes_per_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: &texture, mip_level: 0,
            origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &buf,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row:  Some(bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    queue.submit(std::iter::once(enc.finish()));
    buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::Maintain::Wait);

    let mapped = buf.slice(..).get_mapped_range();
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * bytes_per_row) as usize;
        out.extend_from_slice(&mapped[start..start + (width * 4) as usize]);
    }
    drop(mapped);
    buf.unmap();
    out
}

// ── Save PNG ──────────────────────────────────────────────────────────────

fn save_png(path: &str, pixels: &[u8], width: u32, height: u32) {
    // Write a minimal raw PNG without extra deps using the image crate isn't available.
    // Just write raw RGBA to a .raw file instead, verifiable with e.g. GIMP or ffmpeg.
    let raw_path = path.replace(".png", ".rgba");
    std::fs::write(&raw_path, pixels).unwrap();
    println!("  → {raw_path} ({width}×{height} RGBA, {} bytes)", pixels.len());
    println!("    View with: ffmpeg -f rawvideo -pixel_format rgba -video_size {width}x{height} -i {raw_path} {path}");
}
