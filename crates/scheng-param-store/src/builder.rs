//! `builder.rs` — NodeConfigBuilder: maps ParamStore values into NodeConfigs.
//!
//! The render loop calls `build()` each frame to get the `HashMap<NodeId, NodeConfig>`
//! that `WgpuRuntime::execute_frame` expects.
//!
//! # Routing model
//!
//! Each param definition has an optional `node_label` field.
//! The builder matches node labels to NodeIds via a label→NodeId map
//! that the instrument registers once at startup.
//!
//! Params without a `node_label` are broadcast to all nodes — this lets
//! global params like `u_time_scale` or `u_master_gain` apply everywhere
//! without per-node routing.
//!
//! # Custom frag shaders
//!
//! The builder does NOT set `frag_shader` — shaders are managed by
//! `scheng-hotreload` (or set once at startup). The builder only injects
//! the current param values into NodeConfig::uniforms so they reach the GPU.

use std::collections::HashMap;
use scheng_graph::NodeId;
use crate::NodeConfig;
use crate::ParamStore;

/// Builds `HashMap<NodeId, NodeConfig>` from live ParamStore values.
///
/// Register node labels at startup, then call `build()` each frame.
///
/// ```rust,ignore
/// use scheng_param_store::NodeConfigBuilder;
/// use scheng_graph::NodeId;
///
/// let mut builder = NodeConfigBuilder::new();
/// builder.register("src",  source_node_id);
/// builder.register("proc", process_node_id);
/// builder.register("out",  output_node_id);
///
/// // Each frame:
/// store.step_frame();
/// let configs = builder.build(&store);
/// runtime.execute_frame(&graph, &plan, &configs, &ctx, &mut sink)?;
/// ```
pub struct NodeConfigBuilder {
    /// label → NodeId mapping registered by the instrument.
    label_to_id: HashMap<String, NodeId>,
    /// NodeId → frag shader source (set by hot-reload or startup).
    shaders: HashMap<NodeId, String>,
}

impl NodeConfigBuilder {
    pub fn new() -> Self {
        Self {
            label_to_id: HashMap::new(),
            shaders:     HashMap::new(),
        }
    }

    /// Register a node label → NodeId mapping.
    ///
    /// Labels must match `node_label` in params.json entries.
    pub fn register(&mut self, label: &str, node_id: NodeId) {
        self.label_to_id.insert(label.to_owned(), node_id);
    }

    /// Set or update the fragment shader for a node.
    ///
    /// Called by `scheng-hotreload` when a shader file changes,
    /// or by the instrument at startup.
    pub fn set_shader(&mut self, node_id: NodeId, frag_src: String) {
        self.shaders.insert(node_id, frag_src);
    }

    /// Build `HashMap<NodeId, NodeConfig>` for this frame.
    ///
    /// Call once per frame, after `store.step_frame()`.
    ///
    /// Each NodeConfig receives:
    /// - `frag_shader`: current shader for this node (None = use built-in)
    /// - `uniforms`:    all param values routed to this node
    ///
    /// Routing rules:
    /// - Param with `node_label = Some("proc")` → only NodeId registered as "proc"
    /// - Param with `node_label = None`         → broadcast to every registered node
    pub fn build(&self, store: &ParamStore) -> HashMap<NodeId, NodeConfig> {
        let mut configs: HashMap<NodeId, NodeConfig> = HashMap::new();

        // Seed every registered node with its shader (uniforms start empty).
        for &node_id in self.label_to_id.values() {
            configs.entry(node_id).or_insert_with(|| NodeConfig {
                frag_shader:    self.shaders.get(&node_id).cloned(),
                uniforms:       HashMap::new(),
                output_name:    None,
                input_textures: [None, None, None, None],
                topology:       crate::node_config::PipelineTopology::Fullscreen,
                vertex_data:    None,
                mvp:            None,
            });
        }

        // Route param values into NodeConfig::uniforms.
        for param in &store.schema().params {
            let value = store.get(&param.name).unwrap_or(param.default);

            match &param.node_label {
                // Targeted param — route only to the named node.
                Some(label) => {
                    if let Some(&nid) = self.label_to_id.get(label) {
                        if let Some(config) = configs.get_mut(&nid) {
                            config.uniforms.insert(param.name.clone(), value);
                        }
                        // If label is registered but had no entry yet (shouldn't happen
                        // after the seed loop above), create one.
                        else {
                            let mut cfg = NodeConfig::default();
                            cfg.uniforms.insert(param.name.clone(), value);
                            configs.insert(nid, cfg);
                        }
                    }
                    // Unknown label — log once, don't panic.
                    // (Param registered for a node that wasn't added to this builder.)
                    // Silently skipped; instruments may have optional nodes.
                }
                // Global param — broadcast to every registered node.
                None => {
                    for config in configs.values_mut() {
                        config.uniforms.insert(param.name.clone(), value);
                    }
                }
            }
        }

        configs
    }

    /// Get the NodeId for a registered label. Returns None if unregistered.
    pub fn node_id(&self, label: &str) -> Option<NodeId> {
        self.label_to_id.get(label).copied()
    }

    /// Returns all registered label → NodeId pairs.
    pub fn labels(&self) -> &HashMap<String, NodeId> {
        &self.label_to_id
    }
}

impl Default for NodeConfigBuilder {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ParamDef, ParamSchema};
    use scheng_graph::NodeId;

    fn make_schema_with_routing() -> ParamSchema {
        ParamSchema {
            version: 1,
            params: vec![
                ParamDef {
                    name: "u_brightness".into(), ty: "float".into(),
                    min: 0.0, max: 2.0, default: 1.0, smooth: 0.0,
                    midi_cc: Some(14), midi_channel: None,
                    osc_addr: None,
                    node_label: Some("proc".into()),  // ← targeted
                    description: None,
                },
                ParamDef {
                    name: "u_global_gain".into(), ty: "float".into(),
                    min: 0.0, max: 1.0, default: 0.5, smooth: 0.0,
                    midi_cc: None, midi_channel: None,
                    osc_addr: None,
                    node_label: None,               // ← broadcast
                    description: None,
                },
            ],
        }
    }

    #[test]
    fn targeted_param_routes_only_to_labelled_node() {
        let schema = make_schema_with_routing();
        let store  = ParamStore::new(schema);

        let proc_id = NodeId(1);
        let src_id  = NodeId(0);

        let mut builder = NodeConfigBuilder::new();
        builder.register("src",  src_id);
        builder.register("proc", proc_id);

        let configs = builder.build(&store);

        // u_brightness should be in proc only
        assert!(configs[&proc_id].uniforms.contains_key("u_brightness"),
            "proc should receive u_brightness");
        assert!(!configs[&src_id].uniforms.contains_key("u_brightness"),
            "src should NOT receive u_brightness");
    }

    #[test]
    fn global_param_broadcasts_to_all_nodes() {
        let schema = make_schema_with_routing();
        let store  = ParamStore::new(schema);

        let proc_id = NodeId(1);
        let src_id  = NodeId(0);

        let mut builder = NodeConfigBuilder::new();
        builder.register("src",  src_id);
        builder.register("proc", proc_id);

        let configs = builder.build(&store);

        // u_global_gain should be in both
        assert!(configs[&proc_id].uniforms.contains_key("u_global_gain"));
        assert!(configs[&src_id].uniforms.contains_key("u_global_gain"));
    }

    #[test]
    fn param_value_matches_store_value() {
        let schema = make_schema_with_routing();
        let mut store = ParamStore::new(schema);
        store.set_by_name("u_brightness", 1.75).unwrap();
        store.step_frame();

        let proc_id = NodeId(1);
        let mut builder = NodeConfigBuilder::new();
        builder.register("proc", proc_id);

        let configs = builder.build(&store);
        let v = configs[&proc_id].uniforms["u_brightness"];
        assert!((v - 1.75).abs() < 1e-6, "Expected 1.75, got {v}");
    }

    #[test]
    fn unknown_node_label_is_silently_skipped() {
        let schema = ParamSchema {
            version: 1,
            params: vec![
                ParamDef {
                    name: "u_x".into(), ty: "float".into(),
                    min: 0.0, max: 1.0, default: 0.5, smooth: 0.0,
                    midi_cc: None, midi_channel: None, osc_addr: None,
                    node_label: Some("missing_node".into()),  // not registered
                    description: None,
                },
            ],
        };
        let store = ParamStore::new(schema);
        let mut builder = NodeConfigBuilder::new();
        builder.register("other", NodeId(0));

        // Should not panic
        let configs = builder.build(&store);
        // "other" node gets no u_x (it's targeted at an unregistered label)
        assert!(!configs[&NodeId(0)].uniforms.contains_key("u_x"));
    }
}
