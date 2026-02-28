/// Node Registry — self-describing node type definitions.
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PortDef {
    pub name: &'static str,
    pub dir: &'static str,
    pub kind: &'static str,      // "texture" | "code" | "any"
    pub description: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamDef {
    pub name: &'static str,
    pub kind: &'static str,      // "float" | "shader" | "weights"
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub description: &'static str,
    pub osc_suffix: &'static str, // /scheng/node/{id}/{osc_suffix}
    pub midi_cc_hint: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeDef {
    pub kind: &'static str,
    pub label: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub ports: Vec<PortDef>,
    pub params: Vec<ParamDef>,
    pub default_frag: Option<&'static str>,
    pub is_code_carrier: bool,
}

pub fn all() -> Vec<NodeDef> {
    vec![
        NodeDef {
            kind: "shader_source", label: "ShaderSource", category: "source",
            description: "Carries GLSL code to a ShaderPass via a code edge. Does NOT render pixels itself. Wire 'out' to ShaderPass 'in'. The ShaderPass runs this shader (engine path 3).",
            ports: vec![
                PortDef { name:"out", dir:"out", kind:"code", description:"Code edge → ShaderPass 'in'" },
            ],
            params: vec![
                ParamDef { name:"frag", kind:"shader", min:0.0, max:0.0, default:0.0,
                    description:"GLSL fragment source", osc_suffix:"shader", midi_cc_hint:0 },
            ],
            default_frag: Some(FRAG_COLOURWHEEL),
            is_code_carrier: true,
        },
        NodeDef {
            kind: "shader_pass", label: "ShaderPass", category: "processor",
            description: "Renders a fullscreen GLSL pass to an FBO. Shader from: (1) direct frag on this node, (2) builtin, (3) incoming ShaderSource. Upstream FBOs bound as iChannel0/1/2/3. Can be used standalone — no ShaderSource needed.",
            ports: vec![
                PortDef { name:"in",  dir:"in",  kind:"any",     description:"Texture (iChannel0) or code edge from ShaderSource" },
                PortDef { name:"out", dir:"out", kind:"texture",  description:"This pass's rendered FBO" },
            ],
            params: vec![
                ParamDef { name:"frag", kind:"shader", min:0.0, max:0.0, default:0.0,
                    description:"GLSL fragment source (path 1 — overrides ShaderSource edge)", osc_suffix:"shader", midi_cc_hint:0 },
            ],
            default_frag: Some(FRAG_PASSTHROUGH),
            is_code_carrier: false,
        },
        NodeDef {
            kind: "crossfade", label: "Crossfade", category: "mixer",
            description: "Blends two textures. mix=0→A, mix=1→B. Control live via OSC /scheng/node/{id}/mix or MIDI CC 7.",
            ports: vec![
                PortDef { name:"a",   dir:"in",  kind:"texture", description:"Background (mix=0)" },
                PortDef { name:"b",   dir:"in",  kind:"texture", description:"Foreground (mix=1)" },
                PortDef { name:"out", dir:"out", kind:"texture", description:"Blended output" },
            ],
            params: vec![
                ParamDef { name:"mix", kind:"float", min:0.0, max:1.0, default:0.5,
                    description:"Crossfade (0=A, 1=B)", osc_suffix:"mix", midi_cc_hint:7 },
            ],
            default_frag: None,
            is_code_carrier: false,
        },
        NodeDef {
            kind: "matrix_mix4", label: "MatrixMix4", category: "mixer",
            description: "Weighted blend of up to 4 inputs. Weights are normalised. Control per-channel via OSC.",
            ports: vec![
                PortDef { name:"in0", dir:"in",  kind:"texture", description:"Channel 0" },
                PortDef { name:"in1", dir:"in",  kind:"texture", description:"Channel 1" },
                PortDef { name:"in2", dir:"in",  kind:"texture", description:"Channel 2" },
                PortDef { name:"in3", dir:"in",  kind:"texture", description:"Channel 3" },
                PortDef { name:"out", dir:"out", kind:"texture", description:"Weighted blend" },
            ],
            params: vec![
                ParamDef { name:"weights", kind:"weights", min:0.0, max:1.0, default:0.25,
                    description:"[w0,w1,w2,w3] — send 4 floats via OSC", osc_suffix:"weights", midi_cc_hint:0 },
            ],
            default_frag: None,
            is_code_carrier: false,
        },
        NodeDef {
            kind: "add", label: "Add", category: "mixer",
            description: "Additive saturating blend of two textures. Good for glow/bloom.",
            ports: vec![
                PortDef { name:"a",   dir:"in",  kind:"texture", description:"First input" },
                PortDef { name:"b",   dir:"in",  kind:"texture", description:"Second input" },
                PortDef { name:"out", dir:"out", kind:"texture", description:"A + B (saturated)" },
            ],
            params: vec![], default_frag: None, is_code_carrier: false,
        },
        NodeDef {
            kind: "pixels_out", label: "PixelsOut", category: "output",
            description: "Terminal output. Presents upstream FBO to the render window. Every graph needs exactly one. Must be connected from a ShaderPass or Mixer — not directly from a ShaderSource.",
            ports: vec![
                PortDef { name:"in", dir:"in", kind:"texture", description:"ShaderPass or Mixer output" },
            ],
            params: vec![], default_frag: None, is_code_carrier: false,
        },
        NodeDef {
            kind: "syphon", label: "Syphon", category: "output",
            description: "Shares the rendered frame with other macOS apps via Syphon.",
            ports: vec![
                PortDef { name:"in", dir:"in", kind:"texture", description:"ShaderPass or Mixer output to share" },
            ],
            params: vec![], default_frag: None, is_code_carrier: false,
        },
    ]
}

pub fn find(kind: &str) -> Option<NodeDef> {
    all().into_iter().find(|d| d.kind == kind)
}

pub const FRAG_COLOURWHEEL: &str = r#"#version 330 core
in vec2 v_uv;
out vec4 fragColor;
uniform float u_time;
void main() {
  vec3 col = 0.5 + 0.5 * cos(u_time + v_uv.xyx + vec3(0.0, 2.1, 4.2));
  fragColor = vec4(col, 1.0);
}"#;

pub const FRAG_PASSTHROUGH: &str = r#"#version 330 core
in vec2 v_uv;
out vec4 fragColor;
uniform sampler2D iChannel0;
void main() {
  fragColor = texture(iChannel0, v_uv);
}"#;
