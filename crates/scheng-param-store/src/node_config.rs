//! `NodeConfig` — per-node configuration supplied each frame by the instrument.
//!
//! Lives in scheng-param-store (not scheng-runtime-wgpu) to break the
//! dependency cycle: runtime → param-store → runtime.
//!
//! scheng-runtime-wgpu re-exports this type from its executor module
//! for backward compatibility.

use std::collections::HashMap;

/// Per-node configuration supplied by the instrument each frame.
#[derive(Debug, Clone, Default)]
pub struct NodeConfig {
    /// GLSL 330 fragment shader source. `None` → use built-in for this NodeKind.
    pub frag_shader: Option<String>,
    /// Custom uniform values — maps u_* name → f32 value.
    pub uniforms: HashMap<String, f32>,
    /// Output name for PixelsOut nodes. `None` = primary output.
    pub output_name: Option<String>,
}

impl NodeConfig {
    pub fn set(&mut self, name: &str, value: f32) -> &mut Self {
        self.uniforms.insert(name.to_owned(), value);
        self
    }
}
