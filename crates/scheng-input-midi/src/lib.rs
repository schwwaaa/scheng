//! `scheng-input-midi`
//!
//! MIDI CC input for scheng. Runs on a background thread; writes to
//! `ParamStore` via `Arc<Mutex<ParamStore>>`.
//!
//! # Design (matches shadecore's model)
//!
//! MIDI runs on its own thread — the `midir` callback fires from a platform
//! MIDI thread (CoreMIDI on macOS, WinMM on Windows, ALSA on Linux).
//! The callback acquires a `Mutex<ParamStore>` lock and calls
//! `store.set_by_midi_cc(cc, value)`. Lock contention is negligible
//! (<1µs for a single f32 write at 44100 events/sec worst case).
//!
//! The render loop reads smoothed values from `store.step_frame()` each frame.
//! MIDI messages between frames collapse to "latest value wins" — no queue.
//!
//! # Supported messages
//!
//! - **Control Change (CC)** — primary; maps CC number to param via schema
//! - Note On/Off, Program Change, Pitch Bend — logged but ignored by default
//!   (extend `handle_message` for your instrument)
//!
//! # Quick start
//!
//! ```rust,no_run
//! use std::sync::{Arc, Mutex};
//! use scheng_param_store::ParamStore;
//! use scheng_input_midi::MidiInput;
//!
//! let store = Arc::new(Mutex::new(
//!     ParamStore::from_json_file("assets/params.json").unwrap()
//! ));
//!
//! // Connect to first available MIDI port (or specify by name)
//! let midi = MidiInput::connect_first(Arc::clone(&store)).unwrap();
//!
//! // The connection runs in the background. Keep `midi` alive for the
//! // instrument's lifetime — dropping it disconnects MIDI.
//!
//! // Render loop:
//! // store.lock().unwrap().step_frame();
//! // let configs = builder.build(&store.lock().unwrap());
//! ```

pub mod input;
pub use input::{MidiInput, MidiInputConfig};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MidiError {
    #[error("No MIDI input ports available")]
    NoPorts,

    #[error("MIDI port '{name}' not found")]
    PortNotFound { name: String },

    #[error("Failed to connect to MIDI port: {0}")]
    ConnectionFailed(String),

    #[error("MIDI init failed: {0}")]
    InitFailed(String),
}
