//! `scheng-param-store`
//!
//! The central parameter state for a scheng instrument.
//!
//! # Role in the system
//!
//! ```text
//! assets/params.json  →  ParamSchema::load()
//!                              │
//!                              ▼
//!                         ParamStore
//!                        ┌─────────────────────────────────────────┐
//!  MIDI CC   ──────────▶ │  targets: HashMap<name, f32>            │
//!  OSC msg   ──────────▶ │  (set_by_name / set_by_midi_cc)        │
//!  Hotkey    ──────────▶ │                                         │
//!                        │  values: HashMap<name, f32>             │
//!                        │  (smoothed toward targets each frame)   │
//!                        └─────────────────────────────────────────┘
//!                              │
//!                     step_frame() each frame
//!                              │
//!                              ▼
//!                    build_node_configs()
//!                              │
//!                              ▼
//!              HashMap<NodeId, NodeConfig>  →  WgpuRuntime::execute_frame
//! ```
//!
//! # JSON schema (matches shadecore params.json)
//!
//! ```json
//! {
//!   "version": 1,
//!   "params": [
//!     {
//!       "name": "u_brightness",
//!       "ty": "float",
//!       "min": 0.0,
//!       "max": 2.0,
//!       "default": 1.0,
//!       "smooth": 0.1,
//!       "midi_cc": 14,
//!       "osc_addr": "/scheng/brightness",
//!       "node_label": "proc",
//!       "description": "Overall brightness multiplier"
//!     }
//!   ]
//! }
//! ```
//!
//! # Thread safety
//!
//! `ParamStore` is `Send + Sync`. MIDI and OSC threads call `set_*` methods
//! via `Arc<Mutex<ParamStore>>` or `Arc<RwLock<ParamStore>>`.
//! The render loop calls `step_frame()` and `build_node_configs()` each frame.

pub mod schema;
pub mod store;
pub mod builder;

pub use schema::{ParamDef, ParamSchema};
pub use store::ParamStore;
pub use builder::NodeConfigBuilder;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParamError {
    #[error("Failed to read params file '{path}': {source}")]
    Io { path: String, #[source] source: std::io::Error },

    #[error("Failed to parse params JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Unknown parameter name: '{0}'")]
    UnknownParam(String),

    #[error("Unknown MIDI CC: {0}")]
    UnknownMidiCc(u8),
}
