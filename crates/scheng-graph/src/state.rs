use crate::protocol::*;
use scheng_graph::{Graph, NodeId};
use scheng_runtime_glow::{NodeProps, ShaderSource, FULLSCREEN_VERT};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Per-node metadata
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct NodeMeta {
    pub bridge_id: String,
    pub engine_id: NodeId,
    pub kind:      BridgeNodeKind,
    pub label:     String,
    pub position:  Pos2,
}

// ---------------------------------------------------------------------------
// Shader/param store keyed by bridge_id (stable across rebuilds)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct NodeShaders {
    frag:   HashMap<String, String>,
    vert:   HashMap<String, String>,
    mix:    HashMap<String, scheng_runtime::MixerParams>,
    matrix: HashMap<String, scheng_runtime::MatrixMixParams>,
}

// ---------------------------------------------------------------------------
// BridgeState
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct BridgeState {
    pub graph:   Graph,
    pub nodes:   HashMap<String, NodeMeta>,
    engine_to_bridge: HashMap<NodeId, String>,
    pub edges:   Vec<EdgeDesc>,
    shaders:     NodeShaders,
    pub compiled: bool,
    pub frame:   u64,
}

impl BridgeState {
    pub fn new() -> Self {
        Self {
            graph: Graph::new(),
            nodes: HashMap::new(),
            engine_to_bridge: HashMap::new(),
            edges: Vec::new(),
            shaders: NodeShaders::default(),
            compiled: false,
            frame: 0,
        }
    }

    pub fn add_node(
        &mut self,
        id: String,
        kind: BridgeNodeKind,
        label: String,
        position: Option<Pos2>,
    ) -> Result<NodeDesc, String> {
        if self.nodes.contains_key(&id) {
            return Err(format!("node '{}' already exists", id));
        }
        let engine_id = self.graph.add_node(kind.to_engine());

        // ShaderPass AND ShaderMixN all accept custom GLSL
        let is_shader_node = matches!(kind,
            BridgeNodeKind::ShaderSource | BridgeNodeKind::ShaderPass |
            BridgeNodeKind::ShaderMix2   | BridgeNodeKind::ShaderMix3 |
            BridgeNodeKind::ShaderMix4
        );
        if is_shader_node {
            self.shaders.frag.entry(id.clone()).or_insert_with(default_frag);
        }
        if matches!(kind, BridgeNodeKind::Crossfade) {
            self.shaders.mix.entry(id.clone())
                .or_insert_with(scheng_runtime::MixerParams::default);
        }
        if matches!(kind, BridgeNodeKind::MatrixMix4) {
            self.shaders.matrix.entry(id.clone())
                .or_insert_with(scheng_runtime::MatrixMixParams::default);
        }

        let pos = position.unwrap_or_default();
        self.nodes.insert(id.clone(), NodeMeta {
            bridge_id: id.clone(), engine_id,
            kind: kind.clone(), label: label.clone(), position: pos.clone(),
        });
        self.engine_to_bridge.insert(engine_id, id.clone());
        self.compiled = false;
        Ok(self.make_desc(&id).unwrap())
    }

    pub fn remove_node(&mut self, id: &str) -> Result<(), String> {
        let meta = self.nodes.remove(id)
            .ok_or_else(|| format!("node '{}' not found", id))?;
        self.engine_to_bridge.remove(&meta.engine_id);
        self.edges.retain(|e| e.from_id != id && e.to_id != id);
        self.compiled = false;
        self.rebuild_graph();
        Ok(())
    }

    pub fn connect(
        &mut self,
        from_id: String, from_port: String,
        to_id: String,   to_port: String,
    ) -> Result<EdgeDesc, String> {
        let edge = EdgeDesc {
            from_id: from_id.clone(), from_port: from_port.clone(),
            to_id:   to_id.clone(),   to_port:   to_port.clone(),
        };
        if self.edges.contains(&edge) { return Err("edge already exists".into()); }

        let from_eid = self.nodes.get(&from_id)
            .ok_or_else(|| format!("node '{}' not found", from_id))?.engine_id;
        let to_eid = self.nodes.get(&to_id)
            .ok_or_else(|| format!("node '{}' not found", to_id))?.engine_id;

        self.graph.connect_named(from_eid, &from_port, to_eid, &to_port)
            .map_err(|e| e.to_string())?;

        self.edges.push(edge.clone());
        self.compiled = false;
        Ok(edge)
    }

    pub fn disconnect(&mut self, from_id: &str, from_port: &str, to_id: &str, to_port: &str)
        -> Result<(), String>
    {
        let before = self.edges.len();
        self.edges.retain(|e| !(
            e.from_id == from_id && e.from_port == from_port &&
            e.to_id   == to_id   && e.to_port   == to_port
        ));
        if self.edges.len() == before { return Err("edge not found".into()); }
        self.compiled = false;
        self.rebuild_graph();
        Ok(())
    }

    pub fn set_shader(&mut self, node_id: &str, vert: Option<String>, frag: String)
        -> Result<(), String>
    {
        if !self.nodes.contains_key(node_id) {
            return Err(format!("node '{}' not found", node_id));
        }
        self.shaders.frag.insert(node_id.to_string(), frag);
        if let Some(v) = vert { self.shaders.vert.insert(node_id.to_string(), v); }
        Ok(())
    }

    pub fn set_mix(&mut self, node_id: &str, mix: f32) -> Result<(), String> {
        if !self.nodes.contains_key(node_id) {
            return Err(format!("node '{}' not found", node_id));
        }
        self.shaders.mix.insert(node_id.to_string(), scheng_runtime::MixerParams { mix });
        Ok(())
    }

    pub fn set_weights(&mut self, node_id: &str, weights: [f32; 4]) -> Result<(), String> {
        if !self.nodes.contains_key(node_id) {
            return Err(format!("node '{}' not found", node_id));
        }
        self.shaders.matrix.insert(
            node_id.to_string(),
            scheng_runtime::MatrixMixParams { weights },
        );
        Ok(())
    }

    pub fn move_node(&mut self, id: &str, position: Pos2) -> Result<(), String> {
        let meta = self.nodes.get_mut(id)
            .ok_or_else(|| format!("node '{}' not found", id))?;
        meta.position = position;
        Ok(())
    }

    pub fn snapshot(&self) -> GraphSnapshot {
        GraphSnapshot {
            nodes: self.nodes.keys().filter_map(|id| self.make_desc(id)).collect(),
            edges: self.edges.clone(),
            compiled: self.compiled,
        }
    }

    pub fn build_props(&self) -> NodeProps {
        let mut props = NodeProps::default();
        for (bridge_id, meta) in &self.nodes {
            let eid = meta.engine_id;
            if let Some(frag) = self.shaders.frag.get(bridge_id) {
                let vert = self.shaders.vert.get(bridge_id)
                    .cloned()
                    .unwrap_or_else(|| FULLSCREEN_VERT.to_string());
                props.shader_sources.insert(eid, ShaderSource {
                    vert, frag: frag.clone(),
                    origin: Some(format!("bridge:{}", bridge_id)),
                });
            }
            if let Some(&p) = self.shaders.mix.get(bridge_id) {
                props.mixer_params.insert(eid, p);
            }
            if let Some(&p) = self.shaders.matrix.get(bridge_id) {
                props.matrix_params.insert(eid, p);
            }
        }
        props
    }

    fn make_desc(&self, id: &str) -> Option<NodeDesc> {
        let meta = self.nodes.get(id)?;
        Some(NodeDesc {
            id: id.to_string(),
            kind: meta.kind.clone(),
            label: meta.label.clone(),
            position: meta.position.clone(),
            input_ports:  meta.kind.input_ports().iter().map(|s| s.to_string()).collect(),
            output_ports: meta.kind.output_ports().iter().map(|s| s.to_string()).collect(),
            frag:    self.shaders.frag.get(id).cloned(),
            mix:     self.shaders.mix.get(id).map(|p| p.mix),
            weights: self.shaders.matrix.get(id).map(|p| p.weights),
        })
    }

    fn rebuild_graph(&mut self) {
        self.graph = Graph::new();
        let mut new_ids: HashMap<String, NodeId> = HashMap::new();
        let mut ordered: Vec<&str> = self.nodes.keys().map(|s| s.as_str()).collect();
        ordered.sort();

        for bridge_id in &ordered {
            let meta = self.nodes.get(*bridge_id).unwrap();
            let new_eid = self.graph.add_node(meta.kind.to_engine());
            new_ids.insert((*bridge_id).to_string(), new_eid);
        }
        for (bridge_id, new_eid) in &new_ids {
            if let Some(meta) = self.nodes.get_mut(bridge_id) {
                meta.engine_id = *new_eid;
            }
        }
        self.engine_to_bridge.clear();
        for (bridge_id, meta) in &self.nodes {
            self.engine_to_bridge.insert(meta.engine_id, bridge_id.clone());
        }
        for edge in &self.edges {
            let from_eid = match new_ids.get(&edge.from_id) { Some(&id) => id, None => continue };
            let to_eid   = match new_ids.get(&edge.to_id)   { Some(&id) => id, None => continue };
            let _ = self.graph.connect_named(from_eid, &edge.from_port, to_eid, &edge.to_port);
        }
    }
}

// ---------------------------------------------------------------------------
// RenderSnapshot
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct RenderSnapshot {
    pub graph: Option<Graph>,
    pub plan:  Option<scheng_graph::Plan>,
    pub props: NodeProps,
}

impl RenderSnapshot {
    pub fn empty() -> Self {
        Self { graph: None, plan: None, props: NodeProps::default() }
    }

    pub fn update_from(state: &BridgeState) -> Result<Self, String> {
        let _plan = state.graph.compile().map_err(|e| e.to_string())?;

        let mut render_graph = Graph::new();
        let mut id_map: HashMap<String, NodeId> = HashMap::new();
        let mut ordered: Vec<&str> = state.nodes.keys().map(|s| s.as_str()).collect();
        ordered.sort();

        for bridge_id in &ordered {
            let meta = state.nodes.get(*bridge_id).unwrap();
            let new_eid = render_graph.add_node(meta.kind.to_engine());
            id_map.insert((*bridge_id).to_string(), new_eid);
        }
        for edge in &state.edges {
            let from_eid = match id_map.get(&edge.from_id) { Some(&id) => id, None => continue };
            let to_eid   = match id_map.get(&edge.to_id)   { Some(&id) => id, None => continue };
            let _ = render_graph.connect_named(from_eid, &edge.from_port, to_eid, &edge.to_port);
        }

        let render_plan = render_graph.compile().map_err(|e| e.to_string())?;

        let mut props = NodeProps::default();
        for (bridge_id, new_eid) in &id_map {
            if let Some(frag) = state.shaders.frag.get(bridge_id) {
                let vert = state.shaders.vert.get(bridge_id)
                    .cloned()
                    .unwrap_or_else(|| FULLSCREEN_VERT.to_string());
                props.shader_sources.insert(*new_eid, ShaderSource {
                    vert, frag: frag.clone(),
                    origin: Some(format!("bridge:{}", bridge_id)),
                });
            }
            if let Some(&p) = state.shaders.mix.get(bridge_id) {
                props.mixer_params.insert(*new_eid, p);
            }
            if let Some(&p) = state.shaders.matrix.get(bridge_id) {
                props.matrix_params.insert(*new_eid, p);
            }
        }

        eprintln!("[bridge] compiled: {} nodes, {} edges, {} shader sources",
            render_plan.nodes.len(), render_plan.edges.len(), props.shader_sources.len());

        Ok(Self { graph: Some(render_graph), plan: Some(render_plan), props })
    }
}

// ---------------------------------------------------------------------------

fn default_frag() -> String {
    r#"#version 330 core
in vec2 v_uv;
out vec4 fragColor;
uniform float u_time;
uniform vec2  u_resolution;
uniform sampler2D iChannel0;
void main() {
    vec2 uv = v_uv;
    vec2 grid = floor(uv * 8.0);
    float checker = mod(grid.x + grid.y, 2.0);
    vec3 col = mix(vec3(0.1, 0.1, 0.15), vec3(0.3, 0.5, 1.0), checker);
    col *= 0.7 + 0.3 * sin(u_time * 2.0 + uv.x * 6.28);
    fragColor = vec4(col, 1.0);
}
"#.to_string()
}
