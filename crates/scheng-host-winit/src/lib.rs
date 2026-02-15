//! Host glue (policy layer).
//!
//! This crate will eventually provide winit + GL context creation helpers.
//! It stays separate so the runtime can remain embed-friendly.

pub struct Host;

impl Host {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Host {
    fn default() -> Self {
        Self::new()
    }
}
