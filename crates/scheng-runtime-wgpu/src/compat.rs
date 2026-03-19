//! GLSL 330 → 450 preprocessor for naga compatibility.
//!
//! # Binding layout
//!
//! | binding | resource         |
//! |--------:|-----------------|
//! | 0–3     | iChannel0..3    |
//! | 4       | iSampler        |
//! | 5       | FrameBlock      |
//! | 6       | CustomBlock     | ← Phase 1.2: per-node u_* uniforms

use once_cell::sync::Lazy;
use regex::Regex;

/// Maximum number of custom u_* uniforms per shader.
/// CustomBlock is a fixed-size array of this many f32s.
pub const MAX_CUSTOM_UNIFORMS: usize = 32;

pub const COMPAT_HEADER: &str = r#"#version 450

layout(location = 0) in  vec2 v_uv;
layout(location = 0) out vec4 fragColor;

layout(binding = 0) uniform texture2D iChannel0_tex;
layout(binding = 1) uniform texture2D iChannel1_tex;
layout(binding = 2) uniform texture2D iChannel2_tex;
layout(binding = 3) uniform texture2D iChannel3_tex;
layout(binding = 4) uniform sampler   iSampler;

#define iChannel0 sampler2D(iChannel0_tex, iSampler)
#define iChannel1 sampler2D(iChannel1_tex, iSampler)
#define iChannel2 sampler2D(iChannel2_tex, iSampler)
#define iChannel3 sampler2D(iChannel3_tex, iSampler)

layout(binding = 5) uniform FrameBlock {
    vec2  uResolution;
    float uTime;
    uint  uFrame;
};

#define u_resolution uResolution
#define u_time       uTime
#define u_frame      uFrame

// CustomBlock: per-node user uniforms (binding 6)
// vec4[8] = 8 × 16 bytes = 128 bytes (matches Rust [f32;32] = 128 bytes).
// float[32] would be 32 × 16 = 512 bytes in std140 — do NOT use float arrays.
// Each u_* uniform maps to one component: u_custom[N/4].xyzw[N%4]
layout(binding = 6) uniform CustomBlock {
    vec4 u_custom[8];
};
"#;

/// Result of preprocessing a user fragment shader.
pub struct ProcessedShader {
    /// Full GLSL 450 source ready for naga.
    pub source: String,
    /// Names of custom u_* uniforms found, in declaration order.
    /// Used by the executor to map NodeConfig values → u_custom[] slots.
    pub custom_uniform_names: Vec<String>,
}

// ── Compiled regexes ──────────────────────────────────────────────────────

static RE_VERSION:       Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^[ \t]*#version[^\n]*\n?").unwrap());
static RE_IN_UV:         Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^[ \t]*in\s+vec2\s+v_uv\s*;\s*\n?").unwrap());
static RE_OUT_FRAG:      Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^[ \t]*out\s+vec4\s+fragColor\s*;\s*\n?").unwrap());
static RE_STD_UNIFORMS:  Lazy<Regex> = Lazy::new(|| Regex::new(
    r"(?m)^[ \t]*uniform\s+(?:float\s+u?[Tt]ime|vec2\s+u?[Rr]esolution|(?:int|uint)\s+u?[Ff]rame)\s*;\s*\n?"
).unwrap());
static RE_ICHANNEL_DECL: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^[ \t]*uniform\s+sampler2D\s+iChannel[0-3]\s*;\s*\n?").unwrap());
static RE_CUSTOM_UNIFORM:Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^[ \t]*uniform\s+\w+\s+(u_\w+)\s*;\s*\n?").unwrap());
static RE_ICHANNEL0:     Lazy<Regex> = Lazy::new(|| Regex::new(r"\biChannel0\b").unwrap());
static RE_ICHANNEL1:     Lazy<Regex> = Lazy::new(|| Regex::new(r"\biChannel1\b").unwrap());
static RE_ICHANNEL2:     Lazy<Regex> = Lazy::new(|| Regex::new(r"\biChannel2\b").unwrap());
static RE_ICHANNEL3:     Lazy<Regex> = Lazy::new(|| Regex::new(r"\biChannel3\b").unwrap());

/// Preprocess a GLSL 330 fragment shader for naga + wgpu.
///
/// Custom `u_*` uniforms are:
/// 1. Collected in order → `custom_uniform_names`
/// 2. Their declarations are stripped (CustomBlock provides them)
/// 3. Each name is replaced with `u_custom[N]` in the shader body
pub fn process(user_frag: &str, node_label: &str) -> ProcessedShader {
    let mut src = user_frag.to_owned();

    src = RE_VERSION.replace_all(&src, "").into_owned();
    src = RE_IN_UV.replace_all(&src, "").into_owned();
    src = RE_OUT_FRAG.replace_all(&src, "").into_owned();
    src = RE_STD_UNIFORMS.replace_all(&src, "").into_owned();
    src = RE_ICHANNEL_DECL.replace_all(&src, "").into_owned();

    // Collect custom uniform names in declaration order
    let custom_uniform_names: Vec<String> = RE_CUSTOM_UNIFORM
        .captures_iter(&src)
        .map(|c| c[1].to_owned())
        .collect();

    if custom_uniform_names.len() > MAX_CUSTOM_UNIFORMS {
        log::warn!(
            "[scheng-wgpu] node '{}': {} custom uniforms exceed max {}; extras ignored",
            node_label, custom_uniform_names.len(), MAX_CUSTOM_UNIFORMS
        );
    }

    // Strip all custom uniform declarations (CustomBlock provides them at binding 6)
    src = RE_CUSTOM_UNIFORM.replace_all(&src, "").into_owned();

    // Replace each u_name reference with u_custom[N/4].component
    // e.g. u_brightness → u_custom[0].x, u_contrast → u_custom[0].y, etc.
    // vec4 component names for indices 0..3
    const COMPONENTS: [&str; 4] = ["x", "y", "z", "w"];

    for (idx, name) in custom_uniform_names.iter().enumerate().take(MAX_CUSTOM_UNIFORMS) {
        let vec_idx   = idx / 4;
        let component = COMPONENTS[idx % 4];
        let replacement = format!("u_custom[{}].{}", vec_idx, component);

        let pattern = format!(r"\b{}\b", regex::escape(name));
        if let Ok(re) = Regex::new(&pattern) {
            src = re.replace_all(&src, replacement.as_str()).into_owned();
        }
    }

    // iChannelN rewrite
    src = RE_ICHANNEL0.replace_all(&src, "sampler2D(iChannel0_tex, iSampler)").into_owned();
    src = RE_ICHANNEL1.replace_all(&src, "sampler2D(iChannel1_tex, iSampler)").into_owned();
    src = RE_ICHANNEL2.replace_all(&src, "sampler2D(iChannel2_tex, iSampler)").into_owned();
    src = RE_ICHANNEL3.replace_all(&src, "sampler2D(iChannel3_tex, iSampler)").into_owned();

    let source = format!("{}\n// ── user shader ──\n{}", COMPAT_HEADER, src.trim_start());
    ProcessedShader { source, custom_uniform_names }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_version_and_declarations() {
        let shader = "#version 330 core\nin vec2 v_uv;\nout vec4 fragColor;\nuniform float uTime;\nvoid main() { fragColor = vec4(uTime); }\n";
        let r = process(shader, "test");
        assert!(r.source.contains("#version 450"));
        assert!(!r.source.contains("#version 330"));
        assert!(!r.source.contains("in vec2 v_uv;"));
        assert!(!r.source.contains("\nout vec4 fragColor;"),
            "bare out vec4 fragColor; declaration not stripped");
        assert!(!r.source.contains("uniform float uTime;"));
        assert!(r.source.contains("void main()"));
    }

    #[test]
    fn custom_uniforms_replaced_with_array_access() {
        let shader = "uniform float u_brightness;\nuniform float u_contrast;\nvoid main() { fragColor = vec4(u_brightness, u_contrast, 0.0, 1.0); }\n";
        let r = process(shader, "test");
        assert_eq!(r.custom_uniform_names, vec!["u_brightness", "u_contrast"]);
        // Declarations gone
        assert!(!r.source.contains("uniform float u_brightness;"));
        assert!(!r.source.contains("uniform float u_contrast;"));
        // Usages replaced with vec4 component access
        assert!(r.source.contains("u_custom[0].x"), "u_brightness not replaced: {}", &r.source[r.source.find("user shader").unwrap_or(0)..]);
        assert!(r.source.contains("u_custom[0].y"), "u_contrast not replaced");
        // CustomBlock uses vec4[8] not float[32]
        assert!(r.source.contains("vec4 u_custom[8]"));
    }

    #[test]
    fn custom_uniforms_reported() {
        let shader = "uniform float u_brightness;\nvoid main() { fragColor = vec4(u_brightness); }\n";
        let r = process(shader, "test");
        assert!(r.custom_uniform_names.contains(&"u_brightness".to_owned()));
    }

    #[test]
    fn no_custom_uniforms_still_compiles() {
        let shader = "void main() { fragColor = vec4(v_uv, 0.5 * sin(uTime) + 0.5, 1.0); }\n";
        let r = process(shader, "test");
        assert!(r.custom_uniform_names.is_empty());
        assert!(r.source.contains("void main()"));
    }
}
