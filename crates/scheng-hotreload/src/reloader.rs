//! `reloader.rs` — HotReloader: checks for file changes and applies them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use scheng_graph::NodeId;
use scheng_param_store::{NodeConfigBuilder, ParamStore};

use crate::{watcher::{AssetWatcher, ChangeKind}, HotReloadError};

/// Watches `assets/` and applies live reloads to the instrument state.
///
/// # Reloaded on change
///
/// | File pattern         | Action                                      |
/// |----------------------|---------------------------------------------|
/// | `shaders/*.frag`     | Read new source, call `builder.set_shader()`|
/// | `params.json`        | Call `store.reload_schema()`                |
///
/// # Usage
///
/// ```rust,ignore
/// let mut reloader = HotReloader::new("assets/").unwrap();
///
/// // Register which shader file belongs to which node
/// reloader.register_shader("assets/shaders/proc.frag", proc_node_id);
/// reloader.register_shader("assets/shaders/src.frag",  src_node_id);
///
/// // Each frame (call before building NodeConfigs):
/// reloader.check(&mut builder, &mut store);
/// ```
pub struct HotReloader {
    watcher:       AssetWatcher,
    assets_dir:    PathBuf,
    /// shader path → NodeId — registered by the instrument at startup
    shader_map:    HashMap<PathBuf, NodeId>,
    params_path:   PathBuf,
    reload_count:  u64,
}

impl HotReloader {
    /// Start watching `assets_dir` (e.g. `"assets/"` or `"/path/to/project/assets"`).
    pub fn new(assets_dir: &str) -> Result<Self, HotReloadError> {
        let watcher     = AssetWatcher::new(assets_dir)?;
        let assets_dir  = PathBuf::from(assets_dir).canonicalize()
            .unwrap_or_else(|_| PathBuf::from(assets_dir));
        let params_path = assets_dir.join("params.json");

        Ok(Self {
            watcher,
            assets_dir,
            shader_map:   HashMap::new(),
            params_path,
            reload_count: 0,
        })
    }

    /// Register a shader file path → NodeId.
    ///
    /// When `path` changes, the new source is loaded and
    /// `builder.set_shader(node_id, new_source)` is called.
    ///
    /// `path` can be relative to the assets dir or absolute.
    pub fn register_shader(&mut self, path: &str, node_id: NodeId) {
        let canonical = PathBuf::from(path).canonicalize()
            .unwrap_or_else(|_| PathBuf::from(path));
        self.shader_map.insert(canonical, node_id);
    }

    /// Check for pending file changes and apply them.
    ///
    /// Call once per frame, before building NodeConfigs.
    /// Returns the number of reloads applied this frame (0 = nothing changed).
    pub fn check(
        &mut self,
        builder: &mut NodeConfigBuilder,
        store:   &mut ParamStore,
    ) -> u64 {
        let events = self.watcher.drain();
        if events.is_empty() { return 0; }

        let mut applied = 0u64;
        for event in events {
            // Canonicalize path for lookup
            let canonical = event.path.canonicalize()
                .unwrap_or_else(|_| event.path.clone());

            if event.kind == ChangeKind::Removed { continue; }

            // params.json reload
            if canonical == self.params_path {
                let path_str = self.params_path.to_string_lossy();
                match store.reload_schema(&path_str) {
                    Ok(()) => {
                        log::info!("Hot-reload: params.json reloaded");
                        applied += 1;
                        self.reload_count += 1;
                    }
                    Err(e) => log::warn!("Hot-reload: params.json error: {}", e),
                }
                continue;
            }

            // Shader file reload
            if let Some(&node_id) = self.shader_map.get(&canonical) {
                match read_shader_file(&event.path) {
                    Ok(src) => {
                        log::info!("Hot-reload: shader {:?} → node {:?}", event.path, node_id);
                        builder.set_shader(node_id, src);
                        applied += 1;
                        self.reload_count += 1;
                    }
                    Err(e) => {
                        log::warn!("Hot-reload: failed to read {:?}: {}", event.path, e);
                    }
                }
                continue;
            }

            // Unknown .frag file — log but don't panic
            if event.path.extension().map(|e| e == "frag").unwrap_or(false) {
                log::debug!(
                    "Hot-reload: {:?} changed but not registered (call register_shader first)",
                    event.path
                );
            }
        }

        applied
    }

    /// Total number of successful reloads since startup.
    pub fn reload_count(&self) -> u64 { self.reload_count }

    /// Path to the watched assets directory.
    pub fn assets_dir(&self) -> &Path { &self.assets_dir }
}

fn read_shader_file(path: &Path) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
}
