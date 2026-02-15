use std::fmt;
use std::path::PathBuf;

/// Engine-level errors used across scheng SDK crates.
///
/// Contract rule: this type lives in `scheng-core` and can be re-exported by runtimes.
#[derive(Debug)]
pub enum EngineError {
    // ---- Core / assets / config (SDK-level) ----
    AssetsNotFound {
        start_dir: PathBuf,
    },

    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    Json {
        path: PathBuf,
        source: serde_json::Error,
    },

    JsonValue {
        path: PathBuf,
        source: serde_json::Error,
    },

    InvalidConfig {
        path: PathBuf,
        msg: String,
    },

    // ---- Runtime-facing (backend) ----
    VertexCompile(String),
    FragmentCompile(String),
    Link(String),
    GlCreate(String),

    // ---- Fallback ----
    Other(String),
}

impl EngineError {
    pub fn other<T: Into<String>>(s: T) -> Self {
        EngineError::Other(s.into())
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::AssetsNotFound { start_dir } => {
                write!(f, "assets not found (starting at {})", start_dir.display())
            }
            EngineError::Io { path, source } => {
                write!(f, "io error at {}: {}", path.display(), source)
            }
            EngineError::Json { path, source } => {
                write!(f, "json parse error at {}: {}", path.display(), source)
            }
            EngineError::JsonValue { path, source } => {
                write!(f, "json value error at {}: {}", path.display(), source)
            }
            EngineError::InvalidConfig { path, msg } => {
                write!(f, "invalid config at {}: {}", path.display(), msg)
            }

            EngineError::VertexCompile(msg) => write!(f, "vertex shader compile error: {msg}"),
            EngineError::FragmentCompile(msg) => write!(f, "fragment shader compile error: {msg}"),
            EngineError::Link(msg) => write!(f, "program link error: {msg}"),
            EngineError::GlCreate(msg) => write!(f, "backend object creation failed: {msg}"),

            EngineError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for EngineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EngineError::Io { source, .. } => Some(source),
            EngineError::Json { source, .. } => Some(source),
            EngineError::JsonValue { source, .. } => Some(source),
            _ => None,
        }
    }
}
