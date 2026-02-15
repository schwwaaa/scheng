#![deny(rustdoc::broken_intra_doc_links)]
#![deny(missing_debug_implementations)]

pub mod assets;
pub mod config;
pub mod error;
pub mod events;

// ---- Stable re-exports (only items confirmed to exist) ----
pub use error::EngineError;

// These types are referenced elsewhere in your repo; keep them accessible.
pub use assets::AssetsRoot;

// Config / JSON utilities: re-export the *module* rather than guessing function names.
// This preserves stability and avoids accidental API promises.
pub use config::{
    load_engine_config_from, load_typed_json, parse_loaded_json, ConfigMode, EngineConfig,
    LoadedJson, RenderSelection,
};
