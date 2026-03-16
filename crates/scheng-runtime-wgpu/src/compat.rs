//! GLSL 330 → 450 preprocessor for naga compatibility.

use once_cell::sync::Lazy;
use regex::Regex;

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
"#;

pub struct ProcessedShader {
    pub source:               String,
    pub custom_uniform_names: Vec<String>,
}

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

pub fn process(user_frag: &str, node_label: &str) -> ProcessedShader {
    let mut src = user_frag.to_owned();

    src = RE_VERSION.replace_all(&src, "").into_owned();
    src = RE_IN_UV.replace_all(&src, "").into_owned();
    src = RE_OUT_FRAG.replace_all(&src, "").into_owned();
    src = RE_STD_UNIFORMS.replace_all(&src, "").into_owned();
    src = RE_ICHANNEL_DECL.replace_all(&src, "").into_owned();

    let custom_uniform_names: Vec<String> = RE_CUSTOM_UNIFORM
        .captures_iter(&src)
        .map(|c| c[1].to_owned())
        .collect();

    if !custom_uniform_names.is_empty() {
        log::warn!(
            "[scheng-wgpu] node '{}': custom uniforms {:?} stripped in Phase 1 (default 0.0)",
            node_label, custom_uniform_names
        );
        src = RE_CUSTOM_UNIFORM.replace_all(&src, "").into_owned();
    }

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
    fn custom_uniforms_reported() {
        let shader = "uniform float u_brightness;\nvoid main() { fragColor = vec4(u_brightness); }\n";
        let r = process(shader, "test");
        assert!(r.custom_uniform_names.contains(&"u_brightness".to_owned()));
    }
}
