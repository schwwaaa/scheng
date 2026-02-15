use thiserror::Error;

#[derive(Debug, Error)]
pub enum WebcamError {
    #[error("webcam support not enabled (build with feature: scheng-input-webcam/native)")]
    NotEnabled,

    #[error("{0}")]
    Backend(String),
}

#[derive(Clone, Debug)]
pub struct RgbaFrame {
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>, // RGBA8
}

pub struct Webcam {
    #[cfg(feature = "native")]
    cam: nokhwa::Camera,
}

impl Webcam {
    /// Create webcam `index` and best-effort set the requested resolution.
    pub fn new(index: u32, width: u32, height: u32) -> Result<Self, WebcamError> {
        #[cfg(not(feature = "native"))]
        {
            let _ = (index, width, height);
            return Err(WebcamError::NotEnabled);
        }

        #[cfg(feature = "native")]
        {
            use nokhwa::{
                pixel_format::RgbFormat,
                utils::{CameraIndex, RequestedFormat, RequestedFormatType, Resolution},
                Camera,
            };

            let idx = CameraIndex::Index(index);

            // RequestedFormat is not generic in nokhwa-core 0.1.8; pixel format is via the constructor.
            let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);

            let mut cam =
                Camera::new(idx, requested).map_err(|e| WebcamError::Backend(e.to_string()))?;

            cam.open_stream()
                .map_err(|e| WebcamError::Backend(e.to_string()))?;

            // Best-effort set resolution; do not fail if rejected.
            let _ = cam.set_resolution(Resolution::new(width, height));

            Ok(Self { cam })
        }
    }

    /// Poll the latest decoded frame as RGBA8 bytes.
    pub fn poll_rgba(&mut self) -> Result<RgbaFrame, WebcamError> {
        #[cfg(not(feature = "native"))]
        {
            return Err(WebcamError::NotEnabled);
        }

        #[cfg(feature = "native")]
        {
            use nokhwa::pixel_format::RgbAFormat;

            let buf = self
                .cam
                .frame()
                .map_err(|e| WebcamError::Backend(e.to_string()))?;

            let res = buf.resolution();
            let w = res.width_x;   // already u32
            let h = res.height_y;  // already u32

            let img = buf
                .decode_image::<RgbAFormat>()
                .map_err(|e| WebcamError::Backend(e.to_string()))?;

            let bytes = img.into_raw();

            Ok(RgbaFrame {
                width: w,
                height: h,
                bytes,
            })
        }
    }
}
