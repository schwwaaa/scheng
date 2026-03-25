//! `scheng-input-spout`
//!
//! Spout2 receive → wgpu RGBA texture. Windows only.
//!
//! # Setup
//!
//! Place the Spout2 SDK source at `vendor/Spout2/`:
//!
//! ```text
//! git clone https://github.com/leadedge/Spout2 vendor/Spout2
//! ```
//!
//! Then fill in the TODOs in `native/spout_receiver_bridge.cpp`.
//!
//! # Usage
//!
//! ```rust,ignore
//! use scheng_input_spout::SpoutReceiver;
//!
//! let senders = SpoutReceiver::list_senders();
//! println!("Available: {:?}", senders);
//!
//! let mut recv = SpoutReceiver::connect("OBS", &device, &queue)?;
//!
//! // In render loop
//! recv.poll_with_device(&device, &queue);
//! let view = recv.texture_view();
//! ```

pub mod ffi;
pub mod receiver;

pub use receiver::SpoutReceiver;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SpoutInputError {
    #[error("Spout input is only available on Windows")]
    NotWindows,

    #[error("Spout2 SDK not yet wired — see native/spout_receiver_bridge.cpp TODOs")]
    SdkNotWired,

    #[error("Spout sender '{name}' not found — available: {available:?}")]
    SenderNotFound { name: String, available: Vec<String> },

    #[error("Frame pull failed")]
    PullFailed,
}
