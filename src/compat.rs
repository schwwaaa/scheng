//! `compat.rs` — GLSL preprocessor for the wgpu/naga backend.
//!
//! shadecore and scheng-runtime-glow use GLSL 330 core with OpenGL conventions.
//! naga's GLSL frontend targets GLSL 450 with Vulkan-style explicit bindings.
//!
//! This module bridges the two. It:
//!   1. Strips declarations that the compat header provides
//!   2. Rewrites `iChannel0..3` references to the split texture+sampler form
//!   3. Prepends the full compat header (version, bindings, aliases)
//!
//! # Binding Layout (fixed, matches compat header)
//!
//! | Binding | Resource           | GLSL name           |
//! |--------:|--------------------|---------------------|
//! |       0 | texture2D          | iChannel0_tex       |
//! |       1 | texture2D          | iChannel1_tex       |
//! |       2 | texture2D          | iChannel2_tex       |
//! |       3 | texture2D          | iChannel3_tex       |
//! |       4 | sampler            | iSampler            |
//! |       5 | UniformBuffer      | FrameBlock          |
//! |       6 | UniformBuffer      | CustomBlock (Phase 1.2+) |
//!
//! # Shader compatibility
//!
//! Existing shadecore shaders work unchanged because:
//! - `iChannel0` → macro expands to `sampler2D(iChannel0_tex, iSampler)`
//! - `uTime`, `uResolution`, `uFrame` are in FrameBlock
//! - `u_time`, `u_resolution`, `u_frame` are `#define` aliases

use once_cell::sync::Lazy;
use regex::Regex;

/// The compat header injected at the top of every processed fragment shader.
///
/// It uses GLSL 450 with Vulkan-style layout qualifiers.
/// Combined `sampler2D iChannelN` from GLSL 330 is replaced with
/// separate `texture2D`/`sampler` (Vulkan style) + `#define` aliases.
pub const COMPAT_HEADER: &str = r#"#version 450

// ── scheng-runtime-wgpu compat header ─────────────────────────────────────
// Input from vertex shader
layout(location = 0) in vec2 v_uv;

// Fragment output
layout(location = 0) out vec4 fragColor;

// Input textures — separate texture + shared sampler (naga / Vulkan style).
// iChannel0..3 macros below let existing shaders use them without changes.
layout(binding = 0) uniform texture2D iChannel0_tex;
layout(binding = 1) uniform texture2D iChannel1_tex;
layout(binding = 2) uniform texture2D iChannel2_tex;
layout(binding = 3) uniform texture2D iChannel3_tex;
layout(binding = 4) uniform sampler   iSampler;

// Combined-sampler aliases — texture(iChannel0, v_uv) works as before
#define iChannel0 sampler2D(iChannel0_tex, iSampler)
#define iChannel1 sampler2D(iChannel1_tex, iSampler)
#define iChannel2 sampler2D(iChannel2_tex, iSampler)
#define iChannel3 sampler2D(iChannel3_tex, iSampler)

// Standard frame uniforms
layout(binding = 5) uniform FrameBlock {
    vec2  uResolution;
    float uTime;
    uint  uFrame;
};

// Shadecore-style aliases (both forms work)
#define u_resolution uResolution
#define u_time       uTime
#define u_frame      uFrame
// ── end compat header ──────────────────────────────────────────────────────
"#;

/// Result of preprocessing a user fragment shader.
pub struct ProcessedShader {
    /// The full GLSL 450 source ready for naga compilation.
    pub source: String,
    /// Names of any custom `u_*` uniforms found in the shader.
    /// Phase 1: these are stripped with a warning.
    /// Phase 1.2: they will be packed into CustomBlock (binding 6).
    pub custom_uniform_names: Vec<String>,
}

// ── Compiled regexes (compiled once, reused) ─────────────────────────────

static RE_VERSION: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^[ \t]*#version[^\n]*\n?").unwrap());

static RE_IN_UV: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^[ \t]*in\s+vec2\s+v_uv\s*;\s*\n?").unwrap());

static RE_OUT_FRAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^[ \t]*out\s+vec4\s+fragColor\s*;\s*\n?").unwrap());

// Standard scheng uniforms that the compat header provides in FrameBlock
static RE_STD_UNIFORMS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?m)^[ \t]*uniform\s+(?:float\s+uTime|float\s+u_time|vec2\s+uResolution|vec2\s+u_resolution|(?:int|uint)\s+uFrame|(?:int|uint)\s+u_frame)\s*;\s*\n?",
    )
    .unwrap()
});

// Combined sampler declarations the compat header replaces
static RE_ICHANNEL_DECL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^[ \t]*uniform\s+sampler2D\s+iChannel[0-3]\s*;\s*\n?").unwrap()
});

// iChannelN word-boundary references (usage, not declaration)
static RE_ICHANNEL0: Lazy<Regex> = Lazy::new(|| Regex::new(r"\biChannel0\b").unwrap());
static RE_ICHANNEL1: Lazy<Regex> = Lazy::new(|| Regex::new(r"\biChannel1\b").unwrap());
static RE_ICHANNEL2: Lazy<Regex> = Lazy::new(|| Regex::new(r"\biChannel2\b").unwrap());
static RE_ICHANNEL3: Lazy<Regex> = Lazy::new(|| Regex::new(r"\biChannel3\b").unwrap());

// Custom uniforms: `uniform float u_something;` (Phase 1: strip with warning)
static RE_CUSTOM_UNIFORM: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^[ \t]*uniform\s+\w+\s+(u_\w+)\s*;\s*\n?").unwrap()
});

// ── Public API ────────────────────────────────────────────────────────────

/// Preprocess a user GLSL 330 fragment shader for use with the wgpu backend.
///
/// # Arguments
/// * `user_frag` — the raw fragment shader source as written by the user.
/// * `node_label` — used only in log messages, for diagnostics.
///
/// Returns [`ProcessedShader`] with the ready-for-naga source.
pub fn process(user_frag: &str, node_label: &str) -> ProcessedShader {
    let mut src = user_frag.to_owned();

    // 1. Strip #version (we inject our own)
    src = RE_VERSION.replace_all(&src, "").into_owned();

    // 2. Strip `in vec2 v_uv;` and `out vec4 fragColor;` (compat header provides them)
    src = RE_IN_UV.replace_all(&src, "").into_owned();
    src = RE_OUT_FRAG.replace_all(&src, "").into_owned();

    // 3. Strip standard uniform declarations (they live in FrameBlock now)
    src = RE_STD_UNIFORMS.replace_all(&src, "").into_owned();

    // 4. Strip combined sampler2D iChannelN declarations (compat header provides them)
    src = RE_ICHANNEL_DECL.replace_all(&src, "").into_owned();

    // 5. Collect and strip custom u_* uniforms (Phase 1: warn; Phase 1.2: pack into CustomBlock)
    let custom_uniform_names: Vec<String> = RE_CUSTOM_UNIFORM
        .captures_iter(&src)
        .map(|cap| cap[1].to_owned())
        .collect();

    if !custom_uniform_names.is_empty() {
        log::warn!(
            "[scheng-wgpu] node '{}': custom uniforms {:?} are stripped in Phase 1. \
             They will default to 0.0. Custom uniform support lands in Phase 1.2.",
            node_label,
            custom_uniform_names
        );
        src = RE_CUSTOM_UNIFORM.replace_all(&src, "").into_owned();
    }

    // 6. Rewrite iChannelN usage → sampler2D(iChannelN_tex, iSampler)
    //    These replacements must happen AFTER stripping the declarations above.
    //    Note: the compat header also defines #define aliases, so this step is
    //    belt-and-suspenders — handles cases where naga's #define processing
    //    is incomplete.
    src = RE_ICHANNEL0
        .replace_all(&src, "sampler2D(iChannel0_tex, iSampler)")
        .into_owned();
    src = RE_ICHANNEL1
        .replace_all(&src, "sampler2D(iChannel1_tex, iSampler)")
        .into_owned();
    src = RE_ICHANNEL2
        .replace_all(&src, "sampler2D(iChannel2_tex, iSampler)")
        .into_owned();
    src = RE_ICHANNEL3
        .replace_all(&src, "sampler2D(iChannel3_tex, iSampler)")
        .into_owned();

    // 7. Prepend compat header + user body
    let full_source = format!("{}\n// ── user shader ──\n{}", COMPAT_HEADER, src.trim_start());

    ProcessedShader { source: full_source, custom_uniform_names }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_SHADER: &str = r#"
#version 330 core
in vec2 v_uv;
out vec4 fragColor;
uniform float uTime;
uniform vec2 uResolution;
uniform sampler2D iChannel0;

void main() {
    vec2 uv = v_uv;
    float t = uTime;
    fragColor = texture(iChannel0, uv) + vec4(t, 0.0, 0.0, 0.0);
}
"#;

    #[test]
    fn strips_version_and_declarations() {
        let result = process(SIMPLE_SHADER, "test_node");
        assert!(!result.source.contains("#version 330"), "version not stripped");
        assert!(result.source.contains("#version 450"), "compat version missing");
        assert!(!result.source.contains("in vec2 v_uv;"), "v_uv decl not stripped");
        assert!(!result.source.contains("out vec4 fragColor;"), "fragColor decl not stripped");
        assert!(!result.source.contains("uniform float uTime;"), "uTime decl not stripped");
        assert!(!result.source.contains("uniform sampler2D iChannel0;"), "iChannel0 decl not stripped");
    }

    #[test]
    fn rewrites_ichannel_usage() {
        let result = process(SIMPLE_SHADER, "test_node");
        // After processing, iChannel0 usage should be the expanded form
        assert!(
            result.source.contains("sampler2D(iChannel0_tex, iSampler)"),
            "iChannel0 not rewritten: {}",
            &result.source[..500.min(result.source.len())]
        );
    }

    #[test]
    fn preserves_main_body() {
        let result = process(SIMPLE_SHADER, "test_node");
        assert!(result.source.contains("void main()"), "main() lost");
        assert!(result.source.contains("fragColor ="), "body lost");
    }

    #[test]
    fn custom_uniforms_reported() {
        let shader_with_custom = r#"
#version 330 core
in vec2 v_uv;
out vec4 fragColor;
uniform float u_brightness;
uniform float u_contrast;
void main() { fragColor = vec4(u_brightness, u_contrast, 0.0, 1.0); }
"#;
        let result = process(shader_with_custom, "test_node");
        assert!(result.custom_uniform_names.contains(&"u_brightness".to_owned()));
        assert!(result.custom_uniform_names.contains(&"u_contrast".to_owned()));
    }

    #[test]
    fn minimal_shader_survives() {
        // Bare minimum — no declarations at all
        let bare = r#"
void main() {
    fragColor = vec4(1.0, 0.0, 0.0, 1.0);
}
"#;
        let result = process(bare, "minimal");
        assert!(result.source.contains("void main()"));
        assert!(result.source.contains("fragColor = vec4"));
        assert!(result.custom_uniform_names.is_empty());
    }
}
