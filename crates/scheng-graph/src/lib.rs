#![forbid(unsafe_code)]

//! scheng graph vocabulary and patching model.
//!
//! This crate is **contract-only**: no windowing, no OS policy, no GL handles.
//! It defines an LZX-style mental model: Sources → Processors/Mixers → Outputs.
//!
//! Execution is intentionally minimal in v0: `compile()` returns a lightweight `Plan`
//! that preserves ordering and connectivity information, leaving scheduling/backends
//! to runtime crates.
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(missing_debug_implementations)]

use scheng_core::EngineError;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PortId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortDir {
    In,
    Out,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Endpoint {
    pub node: NodeId,
    pub port: PortId,
    pub dir: PortDir,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub from: Endpoint, // Out
    pub to: Endpoint,   // In
}

/// High-level class of a node in the patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeClass {
    Source,
    Processor,
    Mixer,
    Output,
}

/// A conservative, future-proof node kind.
///
/// Keep this enum small in v0. As the SDK grows, add variants for common building blocks.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeKind {
    // Sources
    ShaderSource,
    NoiseSource,
    PreviousFrame,
    /// Host-provided texture source (GL texture ID supplied via runtime props).
    TextureInputPass,
    VideoDecodeSource,

    // Processors
    ShaderPass,
    ColorCorrect,
    Blur,
    Keyer,
    Feedback,

    // Mixers
    Crossfade,
    Add,
    Multiply,
    KeyMix,
    /// Weighted sum of up to 4 inputs.
    ///
    /// Ports (Option A ordering): in0, in1, in2, in3, out
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
            ShaderSource | NoiseSource | PreviousFrame | TextureInputPass | VideoDecodeSource => NodeClass::Source,
            ShaderPass | ColorCorrect | Blur | Keyer | Feedback => NodeClass::Processor,
            Crossfade | Add | Multiply | KeyMix | MatrixMix4 => NodeClass::Mixer,
            Window | TextureOut | PixelsOut | Syphon | Spout | Recorder | Ndi | Rtsp => {
                NodeClass::Output
            }
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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn add_node(&mut self, kind: NodeKind) -> NodeId {
        let id = NodeId(self.next_node);
        self.next_node += 1;

        // v0: generic port conventions by class; runtime can ignore names if it wants.
        // Some kinds override the generic conventions for semantic ordering.
        let ports = match kind {
            NodeKind::MatrixMix4 => vec![
                self.new_port("in0", PortDir::In),
                self.new_port("in1", PortDir::In),
                self.new_port("in2", PortDir::In),
                self.new_port("in3", PortDir::In),
                self.new_port("out", PortDir::Out),
            ],
            _ => match kind.class() {
                NodeClass::Source => vec![self.new_port("out", PortDir::Out)],
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

        let node = Node { id, kind, ports };
        self.nodes.insert(id, node);
        id
    }

    fn new_port(&mut self, name: &'static str, dir: PortDir) -> Port {
        let id = PortId(self.next_port);
        self.next_port += 1;
        Port { id, name, dir }
    }

    pub fn find_port(&self, node: NodeId, name: &str, dir: PortDir) -> Option<PortId> {
        self.nodes.get(&node).and_then(|n| {
            n.ports
                .iter()
                .find(|p| p.dir == dir && p.name == name)
                .map(|p| p.id)
        })
    }

    /// Connect `from` (Out) → `to` (In).
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

        // v0 safety: ensure the referenced ports actually belong to the specified nodes
        // and match the declared direction.
        {
            let from_ok = self
                .nodes
                .get(&from.node)
                .and_then(|n| n.ports.iter().find(|p| p.id == from.port))
                .is_some();
            if !from_ok {
                return Err(EngineError::other("connect: from port not found on node"));
            }
            let to_ok = self
                .nodes
                .get(&to.node)
                .and_then(|n| n.ports.iter().find(|p| p.id == to.port))
                .is_some();
            if !to_ok {
                return Err(EngineError::other("connect: to port not found on node"));
            }
        }
        // v0: prevent multiple drivers of same input
        if self.edges.iter().any(|e| e.to == to) {
            return Err(EngineError::other("connect: input already connected"));
        }
        self.edges.push(Edge { from, to });
        Ok(())
    }

    /// Convenience: connect by port names using the default conventions.
    pub fn connect_named(
        &mut self,
        from_node: NodeId,
        from_port: &str,
        to_node: NodeId,
        to_port: &str,
    ) -> Result<(), EngineError> {
        let from_pid = self
            .find_port(from_node, from_port, PortDir::Out)
            .ok_or_else(|| EngineError::other("connect_named: from port not found"))?;
        let to_pid = self
            .find_port(to_node, to_port, PortDir::In)
            .ok_or_else(|| EngineError::other("connect_named: to port not found"))?;

        self.connect(
            Endpoint {
                node: from_node,
                port: from_pid,
                dir: PortDir::Out,
            },
            Endpoint {
                node: to_node,
                port: to_pid,
                dir: PortDir::In,
            },
        )
    }

    /// Compile a graph into a lightweight plan. In v0 this is mostly a validation + ordering step.
    pub fn compile(&self) -> Result<Plan, EngineError> {
        // v0: validate all Output nodes have their input connected.
        for n in self.nodes.values() {
            if n.kind.class() == NodeClass::Output {
                let in_port = n.ports.iter().find(|p| p.dir == PortDir::In).map(|p| p.id);
                if let Some(pid) = in_port {
                    let to = Endpoint {
                        node: n.id,
                        port: pid,
                        dir: PortDir::In,
                    };
                    if !self.edges.iter().any(|e| e.to == to) {
                        return Err(EngineError::other("compile: output input not connected"));
                    }
                }
            }
        }

        // v0: emit nodes in insertion order (NodeId sequence) and edges.
        let mut nodes: Vec<NodeId> = self.nodes.keys().copied().collect();
        nodes.sort_by_key(|id| id.0);

        Ok(Plan {
            nodes,
            edges: self.edges.clone(),
        })
    }
}

/// A minimal compiled representation of the graph.
/// Runtimes can interpret this directly or translate it into backend-specific schedules.
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
        let src = g.add_node(NodeKind::ShaderSource);
        let pass = g.add_node(NodeKind::ShaderPass);
        let out = g.add_node(NodeKind::PixelsOut);

        g.connect_named(src, "out", pass, "in").unwrap();
        g.connect_named(pass, "out", out, "in").unwrap();

        let plan = g.compile().unwrap();
        assert!(plan.nodes.len() >= 3);
        assert_eq!(plan.edges.len(), 2);
    }
}
