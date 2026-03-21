//! `builder.rs` — NodeConfigBuilder: maps ParamStore values into NodeConfigs.

use std::collections::HashMap;
use scheng_graph::NodeId;
use crate::{ParamStore, NodeConfig};

pub struct NodeConfigBuilder {
    label_to_id: HashMap<String, NodeId>,
    shaders:     HashMap<NodeId, String>,
}

impl NodeConfigBuilder {
    pub fn new() -> Self {
        Self { label_to_id: HashMap::new(), shaders: HashMap::new() }
    }

    pub fn register(&mut self, label: &str, node_id: NodeId) {
        self.label_to_id.insert(label.to_owned(), node_id);
    }

    pub fn set_shader(&mut self, node_id: NodeId, frag_src: String) {
        self.shaders.insert(node_id, frag_src);
    }

    pub fn build(&self, store: &ParamStore) -> HashMap<NodeId, NodeConfig> {
        let mut configs: HashMap<NodeId, NodeConfig> = HashMap::new();

        for &node_id in self.label_to_id.values() {
            configs.entry(node_id).or_insert_with(|| NodeConfig {
                frag_shader: self.shaders.get(&node_id).cloned(),
                uniforms:    HashMap::new(),
                output_name: None,
            });
        }

        // Inject uniform values from param store into each node's config
        for param in store.schema().params.iter() {
            let value = store.get(&param.name).unwrap_or(param.default);
            match &param.node_label {
                Some(label) => {
                    if let Some(&nid) = self.label_to_id.get(label) {
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

    pub fn node_id(&self, label: &str) -> Option<NodeId> {
        self.label_to_id.get(label).copied()
    }

    pub fn labels(&self) -> &HashMap<String, NodeId> {
        &self.label_to_id
    }

    pub fn reload_count(&self) -> usize { 0 }
}

impl Default for NodeConfigBuilder {
    fn default() -> Self { Self::new() }
}
