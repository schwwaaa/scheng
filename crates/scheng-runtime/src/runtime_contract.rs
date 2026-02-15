use scheng_graph::NodeKind;

/// Returns true if this node kind represents a renderable pass (i.e., it produces pixels by running a shader).
///
/// IMPORTANT: This is the contract between `scheng-graph` and runtime backends.
/// If you add new renderable kinds, update this list.
pub fn is_render_pass(kind: NodeKind) -> bool {
    matches!(kind, NodeKind::ShaderPass)
}

/// Maps an input port name to a texture unit channel index.
///
/// Contract:
/// - runtime backends bind input textures to TEXTURE0 + channel
/// - shaders sample them via uTex{channel} (uTex0, uTex1, ...)
///
/// Expand this if you support more named inputs.
pub fn input_channel_for(kind: NodeKind, port_name: &str) -> Option<u32> {
    // For now we treat all shader-like nodes the same: "in0", "in1", "in2", "in3"
    // and a couple friendly aliases.
    let _ = kind; // reserved for per-kind specialization later

    match port_name {
        "in" | "in0" | "a" | "src" => Some(0),
        "in1" | "b" | "src1" => Some(1),
        "in2" | "c" | "src2" => Some(2),
        "in3" | "d" | "src3" => Some(3),
        _ => None,
    }
}

/// Maps a uniform name to a stable location index.
///
/// Contract:
/// - backends should bind these uniforms consistently (by name or by location if supported)
/// - adding new uniforms is additive; changing/removing is breaking
pub fn uniform_name_is_known(name: &str) -> bool {
    matches!(
        name,
        "uTime" | "uResolution" | "uMouse" | "uSeed" | "uParam0" | "uParam1" | "uParam2" | "uParam3"
    )
}

use std::collections::{HashMap, HashSet};

use scheng_graph::NodeId;

#[derive(Debug, Clone)]
pub struct OutputNamePlan {
    /// The single unnamed PixelsOut node that becomes the engine's primary output ("main").
    pub primary: NodeId,
    /// Explicit outputs: name -> PixelsOut node id
    pub named: HashMap<String, NodeId>,
}

/// Step 5 contract (explicit-only multi-output):
/// - at least 1 PixelsOut exists
/// - exactly 1 PixelsOut must be unnamed (becomes primary/"main")
/// - other PixelsOut nodes must be explicitly named to be addressable
/// - "main" is reserved
/// - explicit names must be unique
pub fn plan_output_names(pixels_out: &[(NodeId, Option<&str>)]) -> Result<OutputNamePlan, String> {
    if pixels_out.is_empty() {
        return Err("no PixelsOut nodes in graph".to_string());
    }

    let mut unnamed: Vec<NodeId> = Vec::new();
    let mut named: HashMap<String, NodeId> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();

    for (id, name) in pixels_out {
        match name {
            None => unnamed.push(*id),
            Some(n) => {
                if *n == "main" {
                    return Err("output name 'main' is reserved".to_string());
                }
                if !seen.insert((*n).to_string()) {
                    return Err(format!("duplicate output name '{n}'"));
                }
                named.insert((*n).to_string(), *id);
            }
        }
    }

    if unnamed.len() != 1 {
        return Err(format!(
            "expected exactly 1 unnamed PixelsOut (primary), found {}",
            unnamed.len()
        ));
    }

    Ok(OutputNamePlan {
        primary: unnamed[0],
        named,
    })
}

#[derive(Debug)]
pub struct BuiltinShader {
    pub vert: String,
    pub frag: String,
}

pub fn builtin_shader_for(kind: NodeKind) -> Option<(String, String)> {
    // Simple full-screen vertex shader
    let vert = r#"
        #version 330 core
        layout(location = 0) in vec2 aPos;
        out vec2 vUV;
        void main() {
            vec2 pos = aPos;
            vUV = 0.5 * (pos + 1.0);
            gl_Position = vec4(pos, 0.0, 1.0);
        }
    "#
    .to_string();

    // Helper: trivial “passthrough” op (copies uTex0)
    fn passthrough(vert: &str) -> BuiltinShader {
        BuiltinShader {
            vert: vert.to_string(),
            frag: r#"
                #version 330 core
                in vec2 vUV;
                out vec4 oColor;
                uniform sampler2D uTex0;
                void main() {
                    oColor = texture(uTex0, vUV);
                }
            "#
            .to_string(),
        }
    }

    match kind {
        // ShaderPass is the generic renderable node.
        NodeKind::ShaderPass => {
            let frag = r#"
                #version 330 core
                in vec2 vUV;
                out vec4 oColor;
                uniform float uTime;
                void main() {
                    float n = fract(sin(dot(vUV * 123.4 + uTime, vec2(127.1, 311.7))) * 43758.5453);
                    oColor = vec4(vec3(n), 1.0);
                }
            "#
            .to_string();
            Some((vert, frag))
        }

        // Default passthrough for other ops for now.
        NodeKind::ColorCorrect | NodeKind::Blur | NodeKind::Keyer => {
            let s = passthrough(&vert);
            Some((s.vert, s.frag))
        }

        _ => None,
    }
}
