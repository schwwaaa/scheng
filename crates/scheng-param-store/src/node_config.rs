//! `NodeConfig` — per-node configuration supplied each frame by the instrument.
//!
//! Lives in scheng-param-store (not scheng-runtime-wgpu) to break the
//! dependency cycle: runtime → param-store → runtime.
//!
//! scheng-runtime-wgpu re-exports this type from its executor module
//! for backward compatibility.

use std::collections::HashMap;
use std::sync::Arc;

// ── PipelineTopology ──────────────────────────────────────────────────────

/// Selects which vertex pipeline a node uses.
///
/// Most nodes use `Fullscreen` — the hardcoded 3-vertex fullscreen triangle
/// with no vertex buffer. Geometry nodes (`MeshSource`, future `MSH3`) use
/// `LineList` or `TriangleList` with an explicit `vertex_data` buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PipelineTopology {
    /// Full-screen triangle. No vertex buffer needed.
    /// This is the default for all shader/processor/mixer nodes.
    #[default]
    Fullscreen,

    /// Explicit vertex list drawn as disconnected triangles.
    /// `vertex_data` must be Some([x,y] pairs in NDC [-1,1]).
    TriangleList,

    /// Explicit vertex list drawn as disconnected line segments.
    /// `vertex_data` must be Some([x,y] pairs in NDC [-1,1]).
    /// **This is the MSH3 topology.**
    LineList,

    /// Each vertex is drawn as a point (1 pixel by default).
    /// `vertex_data` must be Some([x,y] pairs in NDC).
    PointList,
}

impl PipelineTopology {
    /// True for any topology that uses an explicit vertex buffer.
    pub fn is_geometry(self) -> bool {
        !matches!(self, PipelineTopology::Fullscreen)
    }

    /// Maps to the wgpu primitive topology enum.
    pub fn to_wgpu(self) -> wgpu::PrimitiveTopology {
        match self {
            PipelineTopology::Fullscreen    => wgpu::PrimitiveTopology::TriangleList,
            PipelineTopology::TriangleList  => wgpu::PrimitiveTopology::TriangleList,
            PipelineTopology::LineList      => wgpu::PrimitiveTopology::LineList,
            PipelineTopology::PointList     => wgpu::PrimitiveTopology::PointList,
        }
    }
}

// ── NodeConfig ────────────────────────────────────────────────────────────

/// Per-node configuration supplied by the instrument each frame.
#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// GLSL 330 fragment shader source. `None` → use built-in for this NodeKind.
    pub frag_shader: Option<String>,

    /// Custom uniform values — maps u_* name → f32 value.
    /// Populated automatically by `NodeConfigBuilder::build()` from the ParamStore.
    pub uniforms: HashMap<String, f32>,

    /// Output name for PixelsOut nodes. `None` = primary output.
    pub output_name: Option<String>,

    /// Override iChannel0..3 with external textures (webcam, NDI, Syphon receive, etc.)
    /// Graph edges are used as fallback when a slot is None.
    pub input_textures: [Option<Arc<wgpu::Texture>>; 4],

    // ── Geometry fields (only used when topology != Fullscreen) ──────────

    /// Vertex pipeline topology.
    ///
    /// `Fullscreen` (default) — existing 3-vertex triangle, no vertex buffer.
    /// `LineList`             — MSH3 mode; requires `vertex_data`.
    /// `TriangleList`         — custom mesh; requires `vertex_data`.
    pub topology: PipelineTopology,

    /// 2D vertex positions in NDC space [-1, 1].
    ///
    /// Each `[f32; 2]` is (x, y). Z is always 0. W is always 1.
    /// For `LineList`: pairs of vertices define line segments.
    /// For `TriangleList`: triples of vertices define triangles.
    /// Ignored when `topology == Fullscreen`.
    pub vertex_data: Option<Vec<[f32; 2]>>,

    /// Optional MVP (model-view-projection) matrix for geometry nodes.
    ///
    /// Column-major 4×4 f32 matrix, matching WGSL `mat4x4<f32>` layout.
    /// `None` defaults to the identity matrix.
    ///
    /// For 2D video synthesis (MSH3):
    /// - Use an orthographic projection if you want pixel-space coordinates.
    /// - Use the identity matrix if your vertices are already in NDC.
    /// - Rotate/scale the model matrix to animate geometry.
    pub mvp: Option<[[f32; 4]; 4]>,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            frag_shader:    None,
            uniforms:       HashMap::new(),
            output_name:    None,
            input_textures: [None, None, None, None],
            topology:       PipelineTopology::Fullscreen,
            vertex_data:    None,
            mvp:            None,
        }
    }
}

impl NodeConfig {
    /// Set a single custom uniform value. Builder-style, returns `&mut Self`.
    pub fn set(&mut self, name: &str, value: f32) -> &mut Self {
        self.uniforms.insert(name.to_owned(), value);
        self
    }

    /// True if this node needs an explicit vertex buffer (i.e. it's a geometry node).
    pub fn is_geometry(&self) -> bool {
        self.topology.is_geometry()
    }

    /// Returns the MVP matrix, defaulting to identity.
    pub fn mvp_or_identity(&self) -> [[f32; 4]; 4] {
        self.mvp.unwrap_or(IDENTITY_MATRIX)
    }
}

/// Column-major 4×4 identity matrix.
pub const IDENTITY_MATRIX: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_fullscreen() {
        let cfg = NodeConfig::default();
        assert_eq!(cfg.topology, PipelineTopology::Fullscreen);
        assert!(!cfg.is_geometry());
    }

    #[test]
    fn linelist_is_geometry() {
        let cfg = NodeConfig {
            topology: PipelineTopology::LineList,
            ..Default::default()
        };
        assert!(cfg.is_geometry());
    }

    #[test]
    fn mvp_defaults_to_identity() {
        let cfg = NodeConfig::default();
        let identity = IDENTITY_MATRIX;
        assert_eq!(cfg.mvp_or_identity(), identity);
    }

    #[test]
    fn topology_wgpu_mapping() {
        assert_eq!(PipelineTopology::LineList.to_wgpu(),
                   wgpu::PrimitiveTopology::LineList);
        assert_eq!(PipelineTopology::TriangleList.to_wgpu(),
                   wgpu::PrimitiveTopology::TriangleList);
    }
}
