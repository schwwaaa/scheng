//! `NodeConfig` — per-node configuration supplied each frame by the instrument.
//!
//! Lives in scheng-param-store (not scheng-runtime-wgpu) to break the
//! dependency cycle: runtime → param-store → runtime.
//!
//! scheng-runtime-wgpu re-exports this type from its executor module
//! for backward compatibility.
use std::collections::HashMap;
use std::sync::Arc;

/// Per-node configuration supplied by the instrument each frame.
#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// GLSL 330 fragment shader source. `None` → use built-in for this NodeKind.
    pub frag_shader: Option<String>,
    /// Custom uniform values — maps u_* name → f32 value.
    pub uniforms: HashMap<String, f32>,
    /// Output name for PixelsOut nodes. `None` = primary output.
    pub output_name: Option<String>,
    /// Override iChannel0..3 with external textures (webcam, NDI, Syphon receive, etc.)
    /// Graph edges are used as fallback when a slot is None.
    pub input_textures: [Option<Arc<wgpu::Texture>>; 4],
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            frag_shader:    None,
            uniforms:       HashMap::new(),
            output_name:    None,
            input_textures: [None, None, None, None],
        }
    }
}

impl NodeConfig {
    pub fn set(&mut self, name: &str, value: f32) -> &mut Self {
        self.uniforms.insert(name.to_owned(), value);
        self
    }
}
