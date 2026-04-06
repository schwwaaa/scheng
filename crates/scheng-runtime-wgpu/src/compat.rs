//! `compat.rs` — GLSL preprocessor for the wgpu/naga backend.
//!
//! # Binding Layout (fixed, matches compat header)
//!
//! | Binding | Resource           | GLSL name             | Shader stage |
//! |--------:|--------------------|----------------------|-------------|
//! |       0 | texture2D          | iChannel0_tex         | FRAGMENT    |
//! |       1 | texture2D          | iChannel1_tex         | FRAGMENT    |
//! |       2 | texture2D          | iChannel2_tex         | FRAGMENT    |
//! |       3 | texture2D          | iChannel3_tex         | FRAGMENT    |
//! |       4 | sampler            | iSampler              | FRAGMENT    |
//! |       5 | UniformBuffer      | FrameBlock            | FRAGMENT    |
//! |       6 | UniformBuffer      | CustomBlock           | FRAGMENT    |
//! |       7 | UniformBuffer      | MvpBlock (WGSL only)  | VERTEX      |
//!
//! Binding 7 (MvpBlock) lives in the WGSL vertex shader, not in GLSL fragment
//! shaders. Fragment shaders never see it. No compat header changes needed for
//! existing shaders — binding 7 is invisible to the fragment stage.

use once_cell::sync::Lazy;
use regex::Regex;

pub const COMPAT_HEADER: &str = r#"#version 450

// ── scheng-runtime-wgpu compat header ─────────────────────────────────────
layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 fragColor;

// Input textures — separate texture + shared sampler (Vulkan/naga style)
layout(binding = 0) uniform texture2D iChannel0_tex;
layout(binding = 1) uniform texture2D iChannel1_tex;
layout(binding = 2) uniform texture2D iChannel2_tex;
layout(binding = 3) uniform texture2D iChannel3_tex;
layout(binding = 4) uniform sampler   iSampler;

// Combined-sampler aliases — texture(iChannel0, v_uv) works unchanged
#define iChannel0 sampler2D(iChannel0_tex, iSampler)
#define iChannel1 sampler2D(iChannel1_tex, iSampler)
#define iChannel2 sampler2D(iChannel2_tex, iSampler)
#define iChannel3 sampler2D(iChannel3_tex, iSampler)

// Standard frame uniforms (binding 5)
layout(binding = 5) uniform FrameBlock {
    vec2  uResolution;
    float uTime;
    uint  uFrame;
};

// Shadecore-style aliases (both forms work in fragment shaders)
#define u_resolution uResolution
#define u_time       uTime
#define u_frame      uFrame

// NOTE: binding 6 (CustomBlock) is injected below when u_* uniforms are present.
// NOTE: binding 7 (MvpBlock) is VERTEX-stage only — not visible in fragment shaders.

// Audio / FFT spectrum (binding 8) — a 1×N texture, height always 1.
// Sample as: texture(iAudio, vec2(normalized_bin, 0.5)).r
layout(binding = 8) uniform texture2D iAudio_tex;
#define iAudio sampler2D(iAudio_tex, iSampler)
// ── end compat header ──────────────────────────────────────────────────────
"#;

/// Maximum number of custom u_* uniforms packed into CustomBlock.
pub const MAX_CUSTOM_UNIFORMS: usize = 32;

pub struct ProcessedShader {
    pub source:               String,
    pub custom_uniform_names: Vec<String>,
}

// ── Compiled regexes ──────────────────────────────────────────────────────

static RE_VERSION: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^[ \t]*#version[^\n]*\n?").unwrap());

static RE_IN_UV: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^[ \t]*in\s+vec2\s+v_uv\s*;\s*\n?").unwrap());

static RE_OUT_FRAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^[ \t]*out\s+vec4\s+fragColor\s*;\s*\n?").unwrap());

static RE_STD_UNIFORMS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?m)^[ \t]*uniform\s+(?:float\s+uTime|float\s+u_time|vec2\s+uResolution|vec2\s+u_resolution|(?:int|uint)\s+uFrame|(?:int|uint)\s+u_frame)\s*;\s*\n?",
    ).unwrap()
});

static RE_ICHANNEL_DECL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^[ \t]*uniform\s+sampler2D\s+iChannel[0-3]\s*;\s*\n?").unwrap()
});

static RE_ICHANNEL0: Lazy<Regex> = Lazy::new(|| Regex::new(r"\biChannel0\b").unwrap());
static RE_ICHANNEL1: Lazy<Regex> = Lazy::new(|| Regex::new(r"\biChannel1\b").unwrap());
static RE_ICHANNEL2: Lazy<Regex> = Lazy::new(|| Regex::new(r"\biChannel2\b").unwrap());
static RE_ICHANNEL3: Lazy<Regex> = Lazy::new(|| Regex::new(r"\biChannel3\b").unwrap());

static RE_CUSTOM_UNIFORM: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^[ \t]*uniform\s+\w+\s+(u_\w+)\s*;\s*\n?").unwrap()
});

// ── Public API ────────────────────────────────────────────────────────────

pub fn process(user_frag: &str, node_label: &str) -> ProcessedShader {
    let mut src = user_frag.to_owned();

    // Normalize: ensure every uniform declaration starts on its own line.
    // Some shader files pack multiple declarations on one line, e.g.:
    //   "uniform float u_a; uniform float u_b;"
    // The regexes below use ^-anchors (multiline) and only strip declarations
    // that begin at a line boundary. This one replacement handles any packing.
    src = src.replace("; uniform", ";\nuniform");

    src = RE_VERSION.replace_all(&src, "").into_owned();
    src = RE_IN_UV.replace_all(&src, "").into_owned();
    src = RE_OUT_FRAG.replace_all(&src, "").into_owned();
    src = RE_STD_UNIFORMS.replace_all(&src, "").into_owned();
    src = RE_ICHANNEL_DECL.replace_all(&src, "").into_owned();

    let custom_uniform_names: Vec<String> = RE_CUSTOM_UNIFORM
        .captures_iter(&src)
        .map(|cap| cap[1].to_owned())
        .collect();

    if custom_uniform_names.len() > MAX_CUSTOM_UNIFORMS {
        log::warn!(
            "Shader '{}' declares {} custom uniforms but MAX_CUSTOM_UNIFORMS is {}. \
             Uniforms beyond index {} will be ignored.",
            node_label,
            custom_uniform_names.len(),
            MAX_CUSTOM_UNIFORMS,
            MAX_CUSTOM_UNIFORMS - 1,
        );
    }

    if !custom_uniform_names.is_empty() {
        src = RE_CUSTOM_UNIFORM.replace_all(&src, "").into_owned();
        let mut custom_block = String::from("layout(binding = 6) uniform CustomBlock {\n");
        for name in &custom_uniform_names {
            custom_block.push_str(&format!("    float {};\n", name));
        }
        custom_block.push_str("};\n");
        src = format!("{}{}", custom_block, src);
    }

    // Rewrite iChannelN usage → combined sampler form
    src = RE_ICHANNEL0.replace_all(&src, "sampler2D(iChannel0_tex, iSampler)").into_owned();
    src = RE_ICHANNEL1.replace_all(&src, "sampler2D(iChannel1_tex, iSampler)").into_owned();
    src = RE_ICHANNEL2.replace_all(&src, "sampler2D(iChannel2_tex, iSampler)").into_owned();
    src = RE_ICHANNEL3.replace_all(&src, "sampler2D(iChannel3_tex, iSampler)").into_owned();

    let full_source = format!("{}\n// ── user shader ──\n{}", COMPAT_HEADER, src.trim_start());

    ProcessedShader { source: full_source, custom_uniform_names }
}

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
    fragColor = texture(iChannel0, v_uv) + vec4(uTime, 0.0, 0.0, 0.0);
}
"#;

    #[test]
    fn strips_version_and_declarations() {
        let result = process(SIMPLE_SHADER, "test");
        assert!(!result.source.contains("#version 330"));
        assert!(result.source.contains("#version 450"));
        assert!(!result.source.contains("in vec2 v_uv;"));
        assert!(!result.source.contains("out vec4 fragColor;"));
        assert!(!result.source.contains("uniform float uTime;"));
        assert!(!result.source.contains("uniform sampler2D iChannel0;"));
    }

    #[test]
    fn custom_uniforms_get_custom_block() {
        let src = r#"
void main() {
    uniform float u_brightness;
    uniform float u_contrast;
    fragColor = vec4(u_brightness, u_contrast, 0.0, 1.0);
}
"#;
        // Custom uniforms declared inside main() — this tests declaration stripping
        let shader = r#"
#version 330 core
uniform float u_brightness;
uniform float u_contrast;
void main() { fragColor = vec4(u_brightness, u_contrast, 0.0, 1.0); }
"#;
        let result = process(shader, "test");
        assert!(result.custom_uniform_names.contains(&"u_brightness".to_owned()));
        assert!(result.custom_uniform_names.contains(&"u_contrast".to_owned()));
        assert!(result.source.contains("layout(binding = 6) uniform CustomBlock"));
    }

    #[test]
    fn binding_7_not_mentioned_in_fragment_compat_header() {
        // Binding 7 is VERTEX-only. Fragment shaders must never declare it.
        let result = process(SIMPLE_SHADER, "test");
        assert!(
            !result.source.contains("binding = 7"),
            "Fragment compat header must not reference binding 7 (MvpBlock is vertex-only)"
        );
    }
}
