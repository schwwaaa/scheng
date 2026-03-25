//! `scheng-output-spout`
//!
//! Spout2 output sink for scheng. Windows only.
//!
//! Publishes rendered frames to a Spout2 sender, making them available
//! to OBS (Spout2 plugin), TouchDesigner, Resolume, and any other
//! Spout2-capable application on the same Windows machine.
//!
//! # Requirements
//!
//! - Windows only
//! - Visual Studio Build Tools with C++ workload
//! - Copy `native/spout_bridge/` from scheng-runtime-glow into this crate
//!
//! # Status: Phase 3 stub
//!
//! The C interface and OutputSink implementation are defined here.
//! The C++ Spout2 bridge needs to be ported from scheng-runtime-glow's
//! `native/spout_bridge/` directory (it's the proven working implementation).
//!
//! To complete this crate:
//! 1. Copy `crates/scheng-runtime-glow/native/spout_bridge/` to `native/`
//! 2. Uncomment the `[target.windows]` section in build.rs
//! 3. The FFI declarations in ffi.rs match the existing bridge API

#[cfg(target_os = "windows")]
pub mod ffi;
#[cfg(target_os = "windows")]
pub mod sink;

#[cfg(target_os = "windows")]
pub use sink::SpoutSink;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SpoutError {
    #[error("Spout sender creation failed")]
    CreateFailed,

    #[error("Not on Windows — Spout2 is a Windows-only technology")]
    NotWindows,
}
