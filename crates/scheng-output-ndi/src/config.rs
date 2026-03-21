/// Configuration for NdiSink.
#[derive(Debug, Clone)]
pub struct NdiConfig {
    /// NDI source name visible to receivers on the network.
    pub source_name:    String,
    /// NDI group (e.g. "Public"). None = default group.
    pub group:          Option<String>,
    /// Frame rate numerator (e.g. 30000 for 29.97).
    pub framerate_num:  u32,
    /// Frame rate denominator (e.g. 1001 for 29.97, 1 for 30).
    pub framerate_den:  u32,
}

impl Default for NdiConfig {
    fn default() -> Self {
        Self {
            source_name:   "scheng".to_string(),
            group:         None,
            framerate_num: 30,
            framerate_den: 1,
        }
    }
}
