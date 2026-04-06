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
//! the current param values so the upcoming custom uniform support in
//! Phase 1.2 can pick them up.

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
    /// Called once per frame, after `store.step_frame()`.
    ///
    /// Each NodeConfig gets:
    /// - `frag_shader`: current shader for this node (None = use built-in)
    /// - `output_name`: always None for now (primary output)
    ///
    /// NOTE: Custom u_* uniform injection is Phase 1.2. For now, the values
    /// are available here for when that lands — the NodeConfig will gain a
    /// `uniforms: HashMap<String, f32>` field that we populate from the store.
    pub fn build(&self, store: &ParamStore) -> HashMap<NodeId, NodeConfig> {
        let mut configs: HashMap<NodeId, NodeConfig> = HashMap::new();

        // Seed every registered node with its shader.
        for &node_id in self.label_to_id.values() {
            configs.entry(node_id).or_insert_with(|| NodeConfig {
                frag_shader:    self.shaders.get(&node_id).cloned(),
                uniforms:       std::collections::HashMap::new(),
                output_name:    None,
                input_textures: [None, None, None, None],
            });
        }

        // Route param values from the store into each node's uniforms.
        // Params with node_label → that node only.
        // Params with no node_label → broadcast to every registered node.
        for param in &store.schema().params {
            let value = store.get(&param.name).unwrap_or(param.default);
            match &param.node_label {
                Some(label) => {
                    if let Some(&nid) = self.label_to_id.get(label.as_str()) {
                        if let Some(c) = configs.get_mut(&nid) {
                            c.uniforms.insert(param.name.clone(), value);
                        }
                    }
                }
                None => {
                    for c in configs.values_mut() {
                        c.uniforms.insert(param.name.clone(), value);
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
