//! `scheng-hotreload`
//!
//! File system watcher that triggers live reloads without restarting.
//!
//! Watches:
//! - `assets/shaders/*.frag` → recompile shader on next frame
//! - `assets/params.json`    → reload ParamStore schema
//!
//! # Architecture (from shadecore)
//!
//! "Hot-reload checks: watch events set a flag; the redraw tick does
//!  mtimes + reload work."
//!
//! Events come from a platform watcher thread. They set atomic flags.
//! The render loop checks flags each frame and does the actual reload
//! work (reading files, recompiling) on the render thread — no cross-thread
//! GL/wgpu calls.
//!
//! # Quick start
//!
//! ```rust,ignore
//! use scheng_hotreload::HotReloader;
//! use scheng_param_store::NodeConfigBuilder;
//!
//! let mut reloader = HotReloader::new("assets/").unwrap();
//!
//! // In the render loop (each frame):
//! reloader.check(&mut builder, &mut store);
//!
//! // If a shader changed, builder now has the new frag_src for that node.
//! // If params.json changed, store has the reloaded schema.
//! ```

pub mod watcher;
pub mod reloader;

pub use reloader::HotReloader;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HotReloadError {
    #[error("Failed to watch directory '{path}': {source}")]
    Watch { path: String, #[source] source: notify::Error },

    #[error("Failed to read file '{path}': {source}")]
    Read { path: String, #[source] source: std::io::Error },
}
