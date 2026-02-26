/// scheng-bridge protocol
///
/// JSON messages between the browser frontend and the bridge binary.
/// Types here map directly onto the real scheng-graph / scheng-runtime-glow types.

use serde::{Deserialize, Serialize};
// (HashMap used via scheng_runtime_glow re-exports)

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
    MatrixMix4,
    // Outputs
    Window,
    PixelsOut,
    Syphon,
}

impl BridgeNodeKind {
    /// Convert to the real engine type.
    pub fn to_engine(&self) -> scheng_graph::NodeKind {
        use scheng_graph::NodeKind::*;
        match self {
            BridgeNodeKind::ShaderSource     => ShaderSource,
            BridgeNodeKind::NoiseSource      => NoiseSource,
            BridgeNodeKind::TextureInputPass => TextureInputPass,
            BridgeNodeKind::VideoDecodeSource => VideoDecodeSource,
            BridgeNodeKind::ShaderPass       => ShaderPass,
            BridgeNodeKind::ColorCorrect     => ColorCorrect,
            BridgeNodeKind::Blur             => Blur,
            BridgeNodeKind::Keyer            => Keyer,
            BridgeNodeKind::Feedback         => Feedback,
            BridgeNodeKind::Crossfade        => Crossfade,
            BridgeNodeKind::Add              => Add,
            BridgeNodeKind::Multiply         => Multiply,
            BridgeNodeKind::MatrixMix4       => MatrixMix4,
            BridgeNodeKind::Window           => Window,
            BridgeNodeKind::PixelsOut        => PixelsOut,
            BridgeNodeKind::Syphon           => Syphon,
        }
    }

    /// Default input port names for this kind (matches scheng-graph add_node conventions).
    pub fn input_ports(&self) -> Vec<&'static str> {
        use scheng_graph::NodeClass::*;
        match self.to_engine().class() {
            Source    => vec![],
            Processor => vec!["in"],
            Mixer     => {
                if matches!(self, BridgeNodeKind::MatrixMix4) {
                    vec!["in0", "in1", "in2", "in3"]
                } else {
                    vec!["a", "b"]
                }
            }
            Output    => vec!["in"],
        }
    }

    /// Default output port names.
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
    /// Add a node. Returns NodeAdded.
    AddNode {
        /// Caller-chosen stable string ID shown in the UI (e.g. "pass_01").
        id: String,
        kind: BridgeNodeKind,
        label: String,
        position: Option<Pos2>,
    },
    /// Remove a node and all its edges. Returns NodeRemoved.
    RemoveNode { id: String },

    /// Wire an output port to an input port. Returns EdgeAdded.
    Connect {
        from_id: String,
        from_port: String,
        to_id: String,
        to_port: String,
    },
    /// Remove an edge. Returns EdgeRemoved.
    Disconnect {
        from_id: String,
        from_port: String,
        to_id: String,
        to_port: String,
    },

    /// Replace the GLSL fragment source on a ShaderPass or ShaderSource node.
    /// Takes effect on the next frame (lazy recompile). Returns ShaderUpdated.
    SetShader { node_id: String, vert: Option<String>, frag: String },

    /// Set mixer crossfade (0.0–1.0) on a Crossfade node. Returns ParamUpdated.
    SetMix { node_id: String, mix: f32 },

    /// Set all four weights on a MatrixMix4 node. Returns ParamUpdated.
    SetWeights { node_id: String, weights: [f32; 4] },

    /// Recompile the graph topology. Call after any structural change before
    /// advancing frames. Returns Compiled or Error.
    Compile,

    /// Request full state snapshot. Returns State.
    GetState,

    /// Update only the UI position of a node (not forwarded to engine). Returns NodeMoved.
    MoveNode { id: String, position: Pos2 },
}

// ---------------------------------------------------------------------------
// Outbound messages (bridge → frontend)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EngineMsg {
    /// A command succeeded.
    Ok { ack: String },
    /// A command failed.
    Error { message: String },

    /// Full snapshot (sent on connect and after Compile).
    State(GraphSnapshot),

    NodeAdded(NodeDesc),
    NodeRemoved { id: String },
    EdgeAdded(EdgeDesc),
    EdgeRemoved { from_id: String, from_port: String, to_id: String, to_port: String },

    /// Graph compiled successfully.
    Compiled { node_count: usize, edge_count: usize },

    ShaderUpdated { node_id: String },
    ParamUpdated { node_id: String, param: String, value: f32 },
    WeightsUpdated { node_id: String, weights: [f32; 4] },
    NodeMoved { id: String, position: Pos2 },

    /// Emitted each frame when the engine is running (bridge is headless here,
    /// so this is informational — the render window is separate).
    FrameTick { frame: u64, time: f32 },
}

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Pos2 {
    pub x: f32,
    pub y: f32,
}

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
    /// Current frag source if this is a shader node.
    pub frag: Option<String>,
    /// Mix param (Crossfade nodes).
    pub mix: Option<f32>,
    /// Weights (MatrixMix4 nodes).
    pub weights: Option<[f32; 4]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EdgeDesc {
    pub from_id: String,
    pub from_port: String,
    pub to_id: String,
    pub to_port: String,
}
