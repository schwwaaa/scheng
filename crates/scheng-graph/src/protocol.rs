/// scheng-bridge protocol
///
/// JSON messages between the browser frontend and the bridge binary.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Node kind — mirrors scheng_graph::NodeKind exactly
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeNodeKind {
    // Sources
    ShaderSource,
    NoiseSource,
    TextureInputPass,
    VideoDecodeSource,
    // Processors (1 input)
    ShaderPass,
    ColorCorrect,
    Blur,
    Keyer,
    Feedback,
    // NEW: Multi-input custom GLSL shader passes
    // These are Mixer-class so the graph gives them 2/3/4 input ports.
    // Custom GLSL is provided via set_shader — iChannel0/1/2/3 are bound.
    ShaderMix2,   // ports: "a", "b"         → iChannel0, iChannel1
    ShaderMix3,   // ports: "a", "b", "c"    → iChannel0, iChannel1, iChannel2
    ShaderMix4,   // ports: "a", "b", "c","d"→ iChannel0..3
    // Mixers (fixed built-in operations)
    Crossfade,
    Add,
    Multiply,
    MatrixMix4,
    // Outputs
    Window,
    PixelsOut,
    Syphon,
}

impl BridgeNodeKind {
    pub fn to_engine(&self) -> scheng_graph::NodeKind {
        use scheng_graph::NodeKind::*;
        match self {
            BridgeNodeKind::ShaderSource      => ShaderSource,
            BridgeNodeKind::NoiseSource       => NoiseSource,
            BridgeNodeKind::TextureInputPass  => TextureInputPass,
            BridgeNodeKind::VideoDecodeSource => VideoDecodeSource,
            BridgeNodeKind::ShaderPass        => ShaderPass,
            BridgeNodeKind::ColorCorrect      => ColorCorrect,
            BridgeNodeKind::Blur              => Blur,
            BridgeNodeKind::Keyer             => Keyer,
            BridgeNodeKind::Feedback          => Feedback,
            BridgeNodeKind::ShaderMix2        => ShaderMix2,
            BridgeNodeKind::ShaderMix3        => ShaderMix3,
            BridgeNodeKind::ShaderMix4        => ShaderMix4,
            BridgeNodeKind::Crossfade         => Crossfade,
            BridgeNodeKind::Add               => Add,
            BridgeNodeKind::Multiply          => Multiply,
            BridgeNodeKind::MatrixMix4        => MatrixMix4,
            BridgeNodeKind::Window            => Window,
            BridgeNodeKind::PixelsOut         => PixelsOut,
            BridgeNodeKind::Syphon            => Syphon,
        }
    }

    pub fn input_ports(&self) -> Vec<&'static str> {
        use scheng_graph::NodeClass::*;
        match self {
            // Explicit overrides for new multi-input kinds
            BridgeNodeKind::ShaderMix2 => vec!["a", "b"],
            BridgeNodeKind::ShaderMix3 => vec!["a", "b", "c"],
            BridgeNodeKind::ShaderMix4 => vec!["a", "b", "c", "d"],
            BridgeNodeKind::MatrixMix4 => vec!["in0", "in1", "in2", "in3"],
            _ => match self.to_engine().class() {
                Source    => vec![],
                Processor => vec!["in"],
                Mixer     => vec!["a", "b"],
                Output    => vec!["in"],
            },
        }
    }

    pub fn output_ports(&self) -> Vec<&'static str> {
        use scheng_graph::NodeClass::*;
        match self.to_engine().class() {
            Source | Processor | Mixer => vec!["out"],
            Output => vec![],
        }
    }
}

// ---------------------------------------------------------------------------
// Inbound messages (frontend → bridge)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ClientMsg {
    AddNode {
        id: String,
        kind: BridgeNodeKind,
        label: String,
        position: Option<Pos2>,
    },
    RemoveNode { id: String },
    Connect {
        from_id: String,
        from_port: String,
        to_id: String,
        to_port: String,
    },
    Disconnect {
        from_id: String,
        from_port: String,
        to_id: String,
        to_port: String,
    },
    SetShader { node_id: String, vert: Option<String>, frag: String },
    SetMix    { node_id: String, mix: f32 },
    SetWeights{ node_id: String, weights: [f32; 4] },
    Compile,
    GetState,
    MoveNode  { id: String, position: Pos2 },
}

// ---------------------------------------------------------------------------
// Outbound messages (bridge → frontend)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EngineMsg {
    Ok      { ack: String },
    Error   { message: String },
    State(GraphSnapshot),
    NodeAdded(NodeDesc),
    NodeRemoved { id: String },
    EdgeAdded(EdgeDesc),
    EdgeRemoved { from_id: String, from_port: String, to_id: String, to_port: String },
    Compiled { node_count: usize, edge_count: usize },
    ShaderUpdated { node_id: String },
    ParamUpdated  { node_id: String, param: String, value: f32 },
    WeightsUpdated{ node_id: String, weights: [f32; 4] },
    NodeMoved     { id: String, position: Pos2 },
    FrameTick     { frame: u64, time: f32 },
}

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Pos2 { pub x: f32, pub y: f32 }

#[derive(Debug, Clone, Serialize)]
pub struct GraphSnapshot {
    pub nodes: Vec<NodeDesc>,
    pub edges: Vec<EdgeDesc>,
    pub compiled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeDesc {
    pub id: String,
    pub kind: BridgeNodeKind,
    pub label: String,
    pub position: Pos2,
    pub input_ports: Vec<String>,
    pub output_ports: Vec<String>,
    pub frag: Option<String>,
    pub mix: Option<f32>,
    pub weights: Option<[f32; 4]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EdgeDesc {
    pub from_id: String,
    pub from_port: String,
    pub to_id: String,
    pub to_port: String,
}
