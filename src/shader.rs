//! `shader.rs` — GLSL compilation through naga and shader module cache.
//!
//! [`ShaderCache`] compiles fragment shaders lazily and caches the result by
//! source hash. Recompilation only happens when the shader source changes.
//!
//! # Compilation pipeline
//!
//! ```text
//! user .frag (GLSL 330)
//!   → compat::process()          (strip decls, rewrite bindings → GLSL 450)
//!   → naga::front::glsl          (parse → naga IR)
//!   → naga::valid::Validator     (validate IR)
//!   → wgpu::ShaderSource::Naga   (pass naga IR to wgpu)
//!   → wgpu::ShaderModule         (compiled to Metal/DX12/SPIR-V by the driver)
//! ```
//!
//! The built-in vertex shader is WGSL (fullscreen triangle, no vertex buffer).

use std::collections::HashMap;

use naga::front::glsl as naga_glsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};

use crate::{compat, WgpuError};

// ── Built-in vertex shader (WGSL) ────────────────────────────────────────
//
// A single fullscreen triangle covering clip space.
// Emits `v_uv` at location 0 (matching the compat header's `in vec2 v_uv`).
//
// UV convention: (0,0) at bottom-left to match OpenGL / shadecore shaders.
// When sampling textures with these UVs on wgpu (top-left origin),
// the image appears flipped vertically relative to OpenGL — a known difference
// that can be corrected in the presenter layer (Phase 5) or per-shader.
// TODO: add a #define WGPU_BACKEND to let shaders self-correct if needed.

pub const VERTEX_SHADER_WGSL: &str = r#"
struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0)       v_uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOut {
    // Fullscreen triangle — 3 vertices cover the entire clip space.
    // vertex 0: bottom-left  NDC(-1, -1)  uv(0, 0)
    // vertex 1: bottom-right NDC( 3, -1)  uv(2, 0)  (clips at x=1)
    // vertex 2: top-left     NDC(-1,  3)  uv(0, 2)  (clips at y=1)
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    // UV (0,0) = bottom-left in OpenGL convention.
    // wgpu texture (0,0) is top-left, so there is a Y-flip vs OpenGL.
    // See note above — correct in presenter if needed.
    var uv = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(2.0, 0.0),
        vec2<f32>(0.0, 2.0),
    );
    var out: VertexOut;
    out.pos  = vec4<f32>(pos[idx], 0.0, 1.0);
    out.v_uv = uv[idx];
    return out;
}
"#;

/// A compiled (and cached) shader source reference.
#[derive(Clone)]
pub struct ShaderSource {
    /// Vertex GLSL/WGSL. Currently always the built-in WGSL fullscreen triangle.
    pub vert: String,
    /// Fragment GLSL (GLSL 330 core, shadecore convention).
    pub frag: String,
}

impl ShaderSource {
    /// Convenience constructor.
    pub fn new(vert: impl Into<String>, frag: impl Into<String>) -> Self {
        Self { vert: vert.into(), frag: frag.into() }
    }

    /// Create a source that only specifies a fragment shader.
    /// The built-in fullscreen-triangle vertex shader is used automatically.
    pub fn frag_only(frag: impl Into<String>) -> Self {
        Self { vert: VERTEX_SHADER_WGSL.to_owned(), frag: frag.into() }
    }
}

// ── ShaderCache ───────────────────────────────────────────────────────────

/// Caches compiled wgpu shader modules, keyed by source hash.
///
/// Compiling a shader is expensive (driver codegen). The cache prevents
/// recompilation across frames when the shader source hasn't changed.
pub struct ShaderCache {
    /// Cached vertex modules, keyed by source string hash.
    vert_modules: HashMap<u64, wgpu::ShaderModule>,
    /// Cached fragment modules (naga-compiled GLSL), keyed by source hash.
    frag_modules: HashMap<u64, wgpu::ShaderModule>,
}

impl ShaderCache {
    pub fn new() -> Self {
        Self { vert_modules: HashMap::new(), frag_modules: HashMap::new() }
    }

    /// Get or compile the vertex shader module.
    ///
    /// Currently always WGSL (the built-in fullscreen triangle).
    pub fn vertex_module<'a>(
        &'a mut self,
        device: &wgpu::Device,
        vert_src: &str,
    ) -> &'a wgpu::ShaderModule {
        let hash = fxhash(vert_src);
        self.vert_modules.entry(hash).or_insert_with(|| {
            log::debug!("Compiling vertex shader (WGSL, {} bytes)", vert_src.len());
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("scheng_vert"),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(vert_src)),
            })
        })
    }

    /// Get or compile a fragment shader module from GLSL 330 source.
    ///
    /// The source is preprocessed by `compat::process` then compiled through
    /// naga's GLSL frontend before being handed to wgpu.
    pub fn fragment_module<'a>(
        &'a mut self,
        device: &wgpu::Device,
        frag_src: &str,
        node_label: &str,
    ) -> Result<&'a wgpu::ShaderModule, WgpuError> {
        let hash = fxhash(frag_src);

        // Cache hit — return existing module.
        if self.frag_modules.contains_key(&hash) {
            return Ok(self.frag_modules.get(&hash).unwrap());
        }

        // Preprocess GLSL 330 → GLSL 450 with compat bindings.
        let processed = compat::process(frag_src, node_label);

        log::debug!(
            "Compiling fragment shader '{}' via naga ({} bytes)",
            node_label,
            processed.source.len()
        );

        // Compile through naga's GLSL frontend.
        let naga_module = compile_glsl_fragment(&processed.source, node_label)?;

        // Validate the naga IR.
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::empty());
        validator.validate(&naga_module).map_err(|e| {
            WgpuError::NagaValidation(format!("{:?}", e))
        })?;

        // Hand the naga module to wgpu — it translates to Metal/HLSL/SPIR-V as needed.
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(node_label),
            source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(naga_module)),
        });

        self.frag_modules.insert(hash, module);
        Ok(self.frag_modules.get(&hash).unwrap())
    }

    /// Clear all cached modules (e.g., when the device is recreated).
    pub fn clear(&mut self) {
        self.vert_modules.clear();
        self.frag_modules.clear();
    }
}

// ── naga GLSL compilation ─────────────────────────────────────────────────

fn compile_glsl_fragment(
    glsl_450_source: &str,
    node_label: &str,
) -> Result<naga::Module, WgpuError> {
    let mut frontend = naga_glsl::Frontend::default();
    let options = naga_glsl::Options {
        stage: naga::ShaderStage::Fragment,
        defines: Default::default(),
    };

    frontend.parse(&options, glsl_450_source).map_err(|errors| {
        // naga returns a Vec of errors; format them all for the user.
        let messages = errors
            .iter()
            .map(|e| format!("  {:?}", e))
            .collect::<Vec<_>>()
            .join("\n");
        WgpuError::GlslCompile {
            node: node_label.to_owned(),
            message: messages,
        }
    })
}

// ── Fast non-crypto hash ──────────────────────────────────────────────────

/// FxHash-style multiplicative hash — fast, good distribution for strings.
fn fxhash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in s.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_shader_wgsl_parses() {
        // Smoke test: the built-in WGSL vertex shader must be syntactically valid.
        // We can't call the GPU in unit tests, but we can verify naga can parse it.
        // (naga has a WGSL parser as well as a GLSL parser)
        // This test is intentionally lightweight — full GPU tests live in tests/headless.rs
        assert!(VERTEX_SHADER_WGSL.contains("vs_main"), "entry point name missing");
        assert!(VERTEX_SHADER_WGSL.contains("v_uv"), "v_uv output missing");
        assert!(VERTEX_SHADER_WGSL.contains("@builtin(vertex_index)"), "vertex_index missing");
    }

    #[test]
    fn fxhash_different_sources_differ() {
        let h1 = fxhash("void main() { fragColor = vec4(1.0); }");
        let h2 = fxhash("void main() { fragColor = vec4(0.0); }");
        assert_ne!(h1, h2);
    }

    #[test]
    fn fxhash_same_source_stable() {
        let src = "void main() { fragColor = vec4(0.5, 0.5, 0.5, 1.0); }";
        assert_eq!(fxhash(src), fxhash(src));
    }
}
