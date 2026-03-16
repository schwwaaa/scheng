//! `preview.rs` — pixel readback → JPEG → base64 for WebView IPC.

use base64::Engine as _;
use image::{ImageBuffer, Rgb};
use tauri::{AppHandle, Emitter};

pub const PREVIEW_WIDTH:   u32 = 320;
pub const PREVIEW_HEIGHT:  u32 = 180;
pub const PREVIEW_QUALITY: u8  = 75;

/// Encode and emit a preview frame. Infallible — never crashes the render loop.
pub fn emit_preview(app: &AppHandle, pixels: &[u8], width: u32, height: u32) {
    let preview  = downsample_rgba(pixels, width, height, PREVIEW_WIDTH, PREVIEW_HEIGHT);
    let rgb_data: Vec<u8> = preview.chunks(4).flat_map(|px| [px[0], px[1], px[2]]).collect();

    let Some(img) = ImageBuffer::<Rgb<u8>, _>::from_raw(PREVIEW_WIDTH, PREVIEW_HEIGHT, rgb_data)
    else { return; };

    let mut jpeg_bytes = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_bytes, PREVIEW_QUALITY);
    if encoder.encode_image(&img).is_err() { return; }

    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes);
    let _ = app.emit("preview-frame", b64);
}

fn downsample_rgba(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    let out_w = dst_w.min(src_w);
    let out_h = dst_h.min(src_h);
    let mut out = vec![0u8; (out_w * out_h * 4) as usize];
    for dy in 0..out_h {
        for dx in 0..out_w {
            let sx = (dx as u64 * src_w as u64 / out_w as u64) as u32;
            let sy = (dy as u64 * src_h as u64 / out_h as u64) as u32;
            let si = ((sy * src_w + sx) * 4) as usize;
            let di = ((dy * out_w + dx) * 4) as usize;
            if si + 3 < src.len() {
                out[di..di + 4].copy_from_slice(&src[si..si + 4]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downsample_2x2_to_1x1() {
        let src = vec![255,0,0,255, 0,255,0,255, 0,0,255,255, 255,255,0,255];
        let out = downsample_rgba(&src, 2, 2, 1, 1);
        assert_eq!(&out[..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn downsample_passthrough() {
        let src = vec![100,150,200,255, 50,60,70,255];
        let out = downsample_rgba(&src, 2, 1, 2, 1);
        assert_eq!(out, src);
    }
}
