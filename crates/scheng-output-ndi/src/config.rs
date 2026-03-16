//! NDI sender configuration.

use serde::{Deserialize, Serialize};

/// Configuration for the NDI output sink.
///
/// Matches shadecore's `assets/output_ndi.json` schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NdiConfig {
    /// NDI source name visible to receivers. Default: `"scheng"`.
    pub source_name: String,

    /// Group name for NDI discovery. Default: `"Public"`.
    pub group: String,

    /// Send audio (silent track). Set false to disable. Default: false.
    pub send_audio: bool,

    /// Frame rate numerator. Default: 30.
    pub framerate_num: u32,

    /// Frame rate denominator. Default: 1.
    pub framerate_den: u32,
}

impl Default for NdiConfig {
    fn default() -> Self {
        Self {
            source_name:   "scheng".into(),
            group:         "Public".into(),
            send_audio:    false,
            framerate_num: 30,
            framerate_den: 1,
        }
    }
}

impl NdiConfig {
    pub fn from_json_file(path: &str) -> Result<Self, crate::NdiError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| crate::NdiError::SendFailed(e.to_string()))?;
        serde_json::from_str(&text)
            .map_err(|e| crate::NdiError::SendFailed(e.to_string()))
    }
}
