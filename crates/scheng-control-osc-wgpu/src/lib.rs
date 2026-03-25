//! `scheng-control-osc-wgpu`
//!
//! OSC UDP receiver that writes to `ParamStore`. Runs non-blocking
//! via `poll()` called each frame in the render loop.
//!
//! # Address conventions (matches scheng-control-osc and shadecore)
//!
//! Full form:
//!   `/scheng/node/<node_label>/uniform/<param_name>  <float>`
//!
//! Short form (param name only):
//!   `/param/<param_name>  <float>`
//!   `/<param_name>  <float>`
//!
//! Both forms are resolved to the param name in the schema.
//!
//! # Quick start
//!
//! ```rust,ignore
//! use scheng_control_osc_wgpu::OscReceiver;
//!
//! let mut osc = OscReceiver::bind("127.0.0.1:9000").unwrap();
//!
//! // In render loop (each frame):
//! osc.poll(&mut store);
//! store.step_frame();
//! ```
//!
//! # OSC address tooltip convention
//!
//! Each param in params.json should have an `osc_addr` field, e.g.:
//! `"osc_addr": "/scheng/node/proc/uniform/u_brightness"`
//!
//! This address is shown as a tooltip in the editor and used for routing here.

pub mod receiver;
pub use receiver::OscReceiver;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum OscError {
    #[error("Failed to bind UDP socket to '{addr}': {source}")]
    Bind { addr: String, #[source] source: std::io::Error },

    #[error("OSC packet parse error: {0}")]
    Parse(String),
}
