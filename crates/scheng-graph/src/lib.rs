#![forbid(unsafe_code)]

//! scheng graph vocabulary and patching model.
//!
//! LZX-style mental model: Sources → Processors/Mixers → Outputs.

#![deny(rustdoc::broken_intra_doc_links)]
#![deny(missing_debug_implementations)]

use scheng_core::EngineError;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PortId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortDir { In, Out }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Endpoint {
    pub node: NodeId,
    pub port: PortId,
    pub dir: PortDir,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub from: Endpoint,
    pub to: Endpoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeClass { Source, Processor, Mixer, Output }

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeKind {
    // Sources
    ShaderSource,
    NoiseSource,
    PreviousFrame,
    TextureInputPass,
    VideoDecodeSource,

    // Processors (single input "in")
    ShaderPass,
    ColorCorrect,
    Blur,
    Keyer,
    Feedback,

    // --- NEW: Multi-input shader passes ---
    // These are Mixers (so the graph gives them multi-input ports)
    // but they accept custom GLSL via NodeProps::shader_sources,
    // enabling custom mixing/keying/compositing GLSL with 2, 3, or 4 inputs.
    //
    // Ports:  ShaderMix2  → "a", "b"           → iChannel0, iChannel1
    //         ShaderMix3  → "a", "b", "c"      → iChannel0, iChannel1, iChannel2
    //         ShaderMix4  → "a", "b", "c", "d" → iChannel0, iChannel1, iChannel2, iChannel3
    ShaderMix2,
    ShaderMix3,
    ShaderMix4,

    // Mixers (built-in fixed operations)
    Crossfade,
    Add,
    Multiply,
    KeyMix,
    MatrixMix4,

    // Outputs
    Window,
    TextureOut,
    PixelsOut,
    Syphon,
    Spout,
    Recorder,
    Ndi,
    Rtsp,
}

impl NodeKind {
    pub fn class(&self) -> NodeClass {
        use NodeKind::*;
        match self {
            ShaderSource | NoiseSource | PreviousFrame | TextureInputPass | VideoDecodeSource
                => NodeClass::Source,
            ShaderPass | ColorCorrect | Blur | Keyer | Feedback
                => NodeClass::Processor,
            // ShaderMixN are Mixers — this gives them multi-input ports
            ShaderMix2 | ShaderMix3 | ShaderMix4
            | Crossfade | Add | Multiply | KeyMix | MatrixMix4
                => NodeClass::Mixer,
            Window | TextureOut | PixelsOut | Syphon | Spout | Recorder | Ndi | Rtsp
                => NodeClass::Output,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Port {
    pub id: PortId,
    pub name: &'static str,
    pub dir: PortDir,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub ports: Vec<Port>,
}

#[derive(Debug, Default)]
pub struct Graph {
    next_node: u32,
    next_port: u32,
    nodes: HashMap<NodeId, Node>,
    edges: Vec<Edge>,
}

impl Graph {
    pub fn new() -> Self { Self::default() }

    pub fn nodes(&self) -> impl Iterator<Item = &Node> { self.nodes.values() }
    pub fn edges(&self) -> &[Edge] { &self.edges }
    pub fn node(&self, id: NodeId) -> Option<&Node> { self.nodes.get(&id) }

    pub fn add_node(&mut self, kind: NodeKind) -> NodeId {
        let id = NodeId(self.next_node);
        self.next_node += 1;

        let ports = match kind {
            // ShaderMix2: 2 custom-shader inputs "a" and "b"
            NodeKind::ShaderMix2 => vec![
                self.new_port("a", PortDir::In),
                self.new_port("b", PortDir::In),
                self.new_port("out", PortDir::Out),
            ],
            // ShaderMix3: 3 custom-shader inputs "a", "b", "c"
            NodeKind::ShaderMix3 => vec![
                self.new_port("a", PortDir::In),
                self.new_port("b", PortDir::In),
                self.new_port("c", PortDir::In),
                self.new_port("out", PortDir::Out),
            ],
            // ShaderMix4: 4 custom-shader inputs "a", "b", "c", "d"
            NodeKind::ShaderMix4 => vec![
                self.new_port("a", PortDir::In),
                self.new_port("b", PortDir::In),
                self.new_port("c", PortDir::In),
                self.new_port("d", PortDir::In),
                self.new_port("out", PortDir::Out),
            ],
            NodeKind::MatrixMix4 => vec![
                self.new_port("in0", PortDir::In),
                self.new_port("in1", PortDir::In),
                self.new_port("in2", PortDir::In),
                self.new_port("in3", PortDir::In),
                self.new_port("out", PortDir::Out),
            ],
            _ => match kind.class() {
                NodeClass::Source    => vec![self.new_port("out", PortDir::Out)],
                NodeClass::Processor => vec![
                    self.new_port("in", PortDir::In),
                    self.new_port("out", PortDir::Out),
                ],
                NodeClass::Mixer => vec![
                    self.new_port("a", PortDir::In),
                    self.new_port("b", PortDir::In),
                    self.new_port("out", PortDir::Out),
                ],
                NodeClass::Output => vec![self.new_port("in", PortDir::In)],
            },
        };

        self.nodes.insert(id, Node { id, kind, ports });
        id
    }

    fn new_port(&mut self, name: &'static str, dir: PortDir) -> Port {
        let id = PortId(self.next_port);
        self.next_port += 1;
        Port { id, name, dir }
    }

    pub fn find_port(&self, node: NodeId, name: &str, dir: PortDir) -> Option<PortId> {
        self.nodes.get(&node).and_then(|n| {
            n.ports.iter().find(|p| p.dir == dir && p.name == name).map(|p| p.id)
        })
    }

    pub fn connect(&mut self, from: Endpoint, to: Endpoint) -> Result<(), EngineError> {
        if from.dir != PortDir::Out {
            return Err(EngineError::other("connect: from endpoint must be Out"));
        }
        if to.dir != PortDir::In {
            return Err(EngineError::other("connect: to endpoint must be In"));
        }
        if !self.nodes.contains_key(&from.node) || !self.nodes.contains_key(&to.node) {
            return Err(EngineError::other("connect: node not found"));
        }
        let from_ok = self.nodes.get(&from.node)
            .and_then(|n| n.ports.iter().find(|p| p.id == from.port)).is_some();
        if !from_ok { return Err(EngineError::other("connect: from port not found on node")); }
        let to_ok = self.nodes.get(&to.node)
            .and_then(|n| n.ports.iter().find(|p| p.id == to.port)).is_some();
        if !to_ok { return Err(EngineError::other("connect: to port not found on node")); }
        if self.edges.iter().any(|e| e.to == to) {
            return Err(EngineError::other("connect: input already connected"));
        }
        self.edges.push(Edge { from, to });
        Ok(())
    }

    pub fn connect_named(
        &mut self,
        from_node: NodeId, from_port: &str,
        to_node: NodeId,   to_port: &str,
    ) -> Result<(), EngineError> {
        let from_pid = self.find_port(from_node, from_port, PortDir::Out)
            .ok_or_else(|| EngineError::other("connect_named: from port not found"))?;
        let to_pid = self.find_port(to_node, to_port, PortDir::In)
            .ok_or_else(|| EngineError::other("connect_named: to port not found"))?;
        self.connect(
            Endpoint { node: from_node, port: from_pid, dir: PortDir::Out },
            Endpoint { node: to_node,   port: to_pid,   dir: PortDir::In  },
        )
    }

    pub fn compile(&self) -> Result<Plan, EngineError> {
        // Validate: all Output nodes must have their input connected.
        for n in self.nodes.values() {
            if n.kind.class() == NodeClass::Output {
                let in_port = n.ports.iter().find(|p| p.dir == PortDir::In).map(|p| p.id);
                if let Some(pid) = in_port {
                    let to = Endpoint { node: n.id, port: pid, dir: PortDir::In };
                    if !self.edges.iter().any(|e| e.to == to) {
                        return Err(EngineError::other("compile: output input not connected"));
                    }
                }
            }
        }

        // Topological sort via Kahn's algorithm.
        //
        // Builds execution order so every node renders after all its upstream
        // dependencies. Also detects cycles — if any nodes remain after the
        // sort, the graph contains a cycle and compile() returns an error.
        //
        // PreviousFrame is intentionally exempt from cycle detection: it reads
        // the *previous* frame's output, so the cycle is broken by time. We
        // treat its outgoing edges as not creating a dependency for ordering
        // purposes — PreviousFrame is always scheduled first (it's a Source).

        use std::collections::{HashMap as HM, VecDeque};

        // in_degree[node] = number of upstream nodes that must render before it.
        let mut in_degree: HM<NodeId, usize> = self.nodes.keys().map(|&id| (id, 0)).collect();
        // adjacency: node → list of nodes that depend on it.
        let mut adj: HM<NodeId, Vec<NodeId>> = self.nodes.keys().map(|&id| (id, vec![])).collect();

        for edge in &self.edges {
            let from = edge.from.node;
            let to   = edge.to.node;
            // PreviousFrame edges are time-broken — don't count as ordering deps.
            if self.nodes.get(&from).map(|n| n.kind == NodeKind::PreviousFrame).unwrap_or(false) {
                continue;
            }
            adj.entry(from).or_default().push(to);
            *in_degree.entry(to).or_insert(0) += 1;
        }

        // Seed the queue with all nodes that have no upstream dependencies.
        // Use stable ordering within the seed (by NodeId) so compilation is
        // deterministic across runs.
        let mut queue: VecDeque<NodeId> = {
            let mut seeds: Vec<NodeId> = in_degree.iter()
                .filter(|(_, &d)| d == 0)
                .map(|(&id, _)| id)
                .collect();
            seeds.sort_by_key(|id| id.0);
            seeds.into()
        };

        let mut ordered: Vec<NodeId> = Vec::with_capacity(self.nodes.len());

        while let Some(node_id) = queue.pop_front() {
            ordered.push(node_id);
            if let Some(neighbors) = adj.get(&node_id) {
                let mut next: Vec<NodeId> = neighbors.iter().filter_map(|&nb| {
                    let d = in_degree.get_mut(&nb)?;
                    *d -= 1;
                    if *d == 0 { Some(nb) } else { None }
                }).collect();
                next.sort_by_key(|id| id.0); // stable tie-breaking
                queue.extend(next);
            }
        }

        // If not all nodes were visited, the graph has a cycle.
        if ordered.len() != self.nodes.len() {
            return Err(EngineError::other(
                "compile: graph has a cycle — check your connections"
            ));
        }

        Ok(Plan { nodes: ordered, edges: self.edges.clone() })
    }
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub nodes: Vec<NodeId>,
    pub edges: Vec<Edge>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_simple_chain() {
        let mut g = Graph::new();
        let src  = g.add_node(NodeKind::ShaderSource);
        let pass = g.add_node(NodeKind::ShaderPass);
        let out  = g.add_node(NodeKind::PixelsOut);
        g.connect_named(src,  "out", pass, "in").unwrap();
        g.connect_named(pass, "out", out,  "in").unwrap();
        let plan = g.compile().unwrap();
        assert!(plan.nodes.len() >= 3);
        assert_eq!(plan.edges.len(), 2);
    }

    #[test]
    fn shader_mix2_has_two_inputs() {
        let mut g = Graph::new();
        let a   = g.add_node(NodeKind::ShaderSource);
        let b   = g.add_node(NodeKind::ShaderSource);
        let mix = g.add_node(NodeKind::ShaderMix2);
        let out = g.add_node(NodeKind::PixelsOut);
        g.connect_named(a,   "out", mix, "a").unwrap();
        g.connect_named(b,   "out", mix, "b").unwrap();
        g.connect_named(mix, "out", out, "in").unwrap();
        let plan = g.compile().unwrap();
        assert_eq!(plan.edges.len(), 3);
    }
}
