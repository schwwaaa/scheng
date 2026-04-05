//! GLSL → naga → wgpu ShaderModule compilation and cache.
//! Also defines the two WGSL vertex shaders used by the runtime.

use std::collections::HashMap;
use naga::front::glsl as naga_glsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};
use crate::{compat, WgpuError};

// ── Vertex shaders ────────────────────────────────────────────────────────

/// Fullscreen triangle vertex shader (default, no vertex buffer).
///
/// Generates a single full-screen triangle from `vertex_index` alone.
/// No vertex buffer required — 3 draw calls cover the entire NDC square.
pub const VERTEX_SHADER_WGSL: &str = r#"
struct VertexOut {
    @builtin(position)                     pos:  vec4<f32>,
    @location(0) @interpolate(perspective) v_uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOut {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
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

/// Geometry vertex shader (LineList / TriangleList / PointList nodes).
///
/// Reads explicit 2D NDC positions from a vertex buffer (`@location(0)`).
/// Applies the per-node MVP matrix from binding 7 (MvpBlock).
/// Derives UV as `position * 0.5 + 0.5` (NDC → [0,1]).
///
/// # Compat header alignment
///
/// The compat header injects `layout(location = 0) in vec2 v_uv;` for the
/// *fragment* shader. The vertex shader here is a standalone WGSL module
/// that declares its own inputs/outputs — no compat processing needed.
///
/// # MSH3 usage
///
/// ```rust,ignore
/// node_config.topology    = PipelineTopology::LineList;
/// node_config.vertex_data = Some(vec![
///     [-0.5, -0.5],   // line 1 start
///     [ 0.5,  0.5],   // line 1 end
///     [-0.5,  0.5],   // line 2 start
///     [ 0.5, -0.5],   // line 2 end
/// ]);
/// node_config.mvp = Some(rotation_matrix(angle));  // or None for identity
/// // fragment shader: colorize based on v_uv or gl_FragCoord
/// ```
pub const VERTEX_SHADER_GEOMETRY_WGSL: &str = r#"
// ── MvpBlock (binding 7) ──────────────────────────────────────────────────
// Column-major 4×4 matrix. For 2D video synthesis:
//   - Identity (default): NDC positions passed through unchanged.
//   - Ortho projection: map pixel-space coordinates to NDC.
//   - Rotation/scale matrix: animate the geometry each frame.
struct MvpBlock {
    mvp: mat4x4<f32>,
};
@group(0) @binding(7) var<uniform> mvp_block: MvpBlock;

// ── Vertex input / output ─────────────────────────────────────────────────
struct VertexIn {
    @location(0) position: vec2<f32>,   // NDC position [x, y]
};

struct VertexOut {
    @builtin(position)                     pos:  vec4<f32>,
    @location(0) @interpolate(perspective) v_uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    // Apply MVP (identity by default — positions pass through unchanged).
    out.pos  = mvp_block.mvp * vec4<f32>(in.position, 0.0, 1.0);
    // Derive UV from NDC position: NDC [-1,1] → UV [0,1].
    out.v_uv = in.position * 0.5 + vec2<f32>(0.5, 0.5);
    return out;
}
"#;

// ── ShaderSource ──────────────────────────────────────────────────────────

/// A compiled shader source reference (used by the glow backend and bridge).
#[derive(Clone)]
pub struct ShaderSource {
    pub vert: String,
    pub frag: String,
}

impl ShaderSource {
    pub fn frag_only(frag: impl Into<String>) -> Self {
        Self { vert: VERTEX_SHADER_WGSL.to_owned(), frag: frag.into() }
    }
}

// ── ShaderCache ───────────────────────────────────────────────────────────

/// Caches compiled wgpu shader modules + their custom uniform name lists.
pub struct ShaderCache {
    frag_modules: HashMap<u64, (wgpu::ShaderModule, Vec<String>)>,
}

impl ShaderCache {
    pub fn new() -> Self {
        Self { frag_modules: HashMap::new() }
    }

    pub fn fragment_module_with_names<'a>(
        &'a mut self,
        device:     &wgpu::Device,
        frag_src:   &str,
        node_label: &str,
    ) -> Result<(&'a wgpu::ShaderModule, Vec<String>), WgpuError> {
        let hash = fxhash(frag_src);

        if !self.frag_modules.contains_key(&hash) {
            let processed = compat::process(frag_src, node_label);
            log::debug!(
                "Compiling shader '{}' via naga ({} bytes)",
                node_label, processed.source.len()
            );

            let naga_module = compile_glsl_fragment(&processed.source, node_label)?;
            let mut v = Validator::new(ValidationFlags::all(), Capabilities::empty());
            v.validate(&naga_module)
                .map_err(|e| WgpuError::NagaValidation(format!("{e:?}")))?;

            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label:  Some(node_label),
                source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(naga_module)),
            });
            self.frag_modules.insert(hash, (module, processed.custom_uniform_names));
        }

        let (module, names) = self.frag_modules.get(&hash).unwrap();
        Ok((module, names.clone()))
    }

    pub fn clear(&mut self) { self.frag_modules.clear(); }
}

fn compile_glsl_fragment(source: &str, node_label: &str) -> Result<naga::Module, WgpuError> {
    let mut fe   = naga_glsl::Frontend::default();
    let opts     = naga_glsl::Options {
        stage:   naga::ShaderStage::Fragment,
        defines: Default::default(),
    };
    fe.parse(&opts, source).map_err(|errors| {
        let messages = errors.errors.iter()
            .map(|e| format!("  {e:?}"))
            .collect::<Vec<_>>()
            .join("\n");
        WgpuError::GlslCompile { node: node_label.to_owned(), message: messages }
    })
}

fn fxhash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in s.as_bytes() { h ^= b as u64; h = h.wrapping_mul(0x100000001b3); }
    h
}
