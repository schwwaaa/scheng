//! `input.rs` — MidiInput: connects to a MIDI port and forwards CC to ParamStore.

use std::sync::{Arc, Mutex};

use midir::{MidiInput as MidirInput, MidiInputConnection};
use scheng_param_store::ParamStore;

use crate::MidiError;

/// Configuration for the MIDI input connection.
#[derive(Debug, Clone)]
pub struct MidiInputConfig {
    /// MIDI port name to connect to.
    /// Use `MidiInput::list_ports()` to see available ports.
    /// If `None`, connects to the first available port.
    pub port_name: Option<String>,

    /// MIDI channel filter [1, 16]. `None` = omni (accept all channels).
    pub channel: Option<u8>,

    /// Client name shown in the system MIDI routing (e.g. DAW MIDI routing).
    pub client_name: String,
}

impl Default for MidiInputConfig {
    fn default() -> Self {
        Self {
            port_name:   None,
            channel:     None,
            client_name: "scheng".into(),
        }
    }
}

/// An active MIDI input connection.
///
/// Runs on a background thread. Keep alive for the instrument's lifetime.
/// Dropping disconnects MIDI automatically.
pub struct MidiInput {
    /// The live midir connection. Held to keep the callback alive.
    _connection: MidiInputConnection<()>,
    port_name:   String,
}

impl MidiInput {
    /// Connect to the first available MIDI input port.
    pub fn connect_first(store: Arc<Mutex<ParamStore>>) -> Result<Self, MidiError> {
        Self::connect(MidiInputConfig::default(), store)
    }

    /// Connect using explicit config.
    pub fn connect(
        config: MidiInputConfig,
        store:  Arc<Mutex<ParamStore>>,
    ) -> Result<Self, MidiError> {
        let midi_in = MidirInput::new(&config.client_name)
            .map_err(|e| MidiError::InitFailed(e.to_string()))?;

        let ports = midi_in.ports();
        if ports.is_empty() {
            return Err(MidiError::NoPorts);
        }

        // Find the requested port or default to first.
        let port = match &config.port_name {
            Some(name) => {
                ports.iter().find(|p| {
                    midi_in.port_name(p)
                        .map(|n| n.contains(name.as_str()))
                        .unwrap_or(false)
                })
                .ok_or_else(|| MidiError::PortNotFound { name: name.clone() })?
            }
            None => &ports[0],
        };

        let port_name = midi_in.port_name(port)
            .unwrap_or_else(|_| "unknown".into());

        let channel_filter = config.channel;

        let connection = midi_in
            .connect(
                port,
                "scheng-midi-recv",
                move |_timestamp_us, message, _| {
                    handle_message(message, channel_filter, &store);
                },
                (),
            )
            .map_err(|e| MidiError::ConnectionFailed(e.to_string()))?;

        log::info!("MIDI connected: '{}'", port_name);

        Ok(Self { _connection: connection, port_name })
    }

    /// List all available MIDI input port names.
    pub fn list_ports() -> Result<Vec<String>, MidiError> {
        let midi_in = MidirInput::new("scheng-list")
            .map_err(|e| MidiError::InitFailed(e.to_string()))?;
        Ok(midi_in.ports().iter()
            .filter_map(|p| midi_in.port_name(p).ok())
            .collect())
    }

    /// The name of the connected port.
    pub fn port_name(&self) -> &str { &self.port_name }
}

// ── MIDI message handling ─────────────────────────────────────────────────

/// Parse a raw MIDI message and update the ParamStore.
///
/// Called from the midir background thread — must be fast and non-blocking.
/// The Mutex lock is held only for the duration of a single `set_by_midi_cc` call.
fn handle_message(msg: &[u8], channel_filter: Option<u8>, store: &Arc<Mutex<ParamStore>>) {
    if msg.is_empty() { return; }

    let status  = msg[0];
    let msg_type = status & 0xF0;
    let channel  = (status & 0x0F) + 1; // MIDI channels are 1-indexed in user-facing code

    // Apply channel filter (omni if None)
    if let Some(filter_ch) = channel_filter {
        if channel != filter_ch { return; }
    }

    match msg_type {
        // Control Change (CC)
        0xB0 if msg.len() >= 3 => {
            let cc    = msg[1];
            let value = msg[2]; // raw 0–127

            log::trace!("MIDI CC ch={} cc={} val={}", channel, cc, value);

            match store.lock() {
                Ok(mut s) => {
                    if let Err(e) = s.set_by_midi_cc(cc, value) {
                        // Unknown CC — not an error if the schema just doesn't map it
                        log::trace!("MIDI CC {}: {}", cc, e);
                    }
                }
                Err(e) => log::error!("MIDI: ParamStore lock poisoned: {}", e),
            }
        }

        // Note On — log but ignore (extend for note-triggered params)
        0x90 if msg.len() >= 3 => {
            let note     = msg[1];
            let velocity = msg[2];
            log::trace!("MIDI Note On ch={} note={} vel={}", channel, note, velocity);
        }

        // Note Off
        0x80 if msg.len() >= 3 => {
            log::trace!("MIDI Note Off ch={} note={}", channel, msg[1]);
        }

        // Pitch Bend
        0xE0 if msg.len() >= 3 => {
            let lsb = msg[1] as u16;
            let msb = msg[2] as u16;
            let raw = (msb << 7) | lsb; // 0–16383, center = 8192
            log::trace!("MIDI Pitch Bend ch={} val={}", channel, raw);
        }

        // Program Change
        0xC0 if msg.len() >= 2 => {
            log::trace!("MIDI Program Change ch={} prog={}", channel, msg[1]);
        }

        _ => {
            log::trace!("MIDI unhandled: {:02X?}", msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scheng_param_store::{ParamSchema, ParamStore};
    use scheng_param_store::schema::ParamDef;

    fn make_store_with_cc14() -> Arc<Mutex<ParamStore>> {
        let schema = ParamSchema {
            version: 1,
            params: vec![
                ParamDef {
                    name: "u_bright".into(), ty: "float".into(),
                    min: 0.0, max: 2.0, default: 1.0, smooth: 0.0,
                    midi_cc: Some(14), midi_channel: None,
                    osc_addr: None, node_label: None, description: None,
                },
            ],
        };
        Arc::new(Mutex::new(ParamStore::new(schema)))
    }

    #[test]
    fn cc_message_updates_store() {
        let store = make_store_with_cc14();

        // Simulate MIDI CC 14, value 127 → u_bright should hit max (2.0)
        let msg: &[u8] = &[0xB0, 14, 127]; // ch1, CC14, val127
        handle_message(msg, None, &store);

        let mut s = store.lock().unwrap();
        s.step_frame();
        let v = s.get("u_bright").unwrap();
        assert!((v - 2.0).abs() < 0.01, "Expected ~2.0, got {v}");
    }

    #[test]
    fn cc_message_channel_filter_respected() {
        let store = make_store_with_cc14();

        // Send on ch2, filter is ch1 — should be ignored
        let msg: &[u8] = &[0xB1, 14, 127]; // ch2
        handle_message(msg, Some(1), &store); // filter = ch1

        let mut s = store.lock().unwrap();
        s.step_frame();
        let v = s.get("u_bright").unwrap();
        assert_eq!(v, 1.0, "Default should be unchanged when channel filtered");
    }

    #[test]
    fn unknown_cc_does_not_panic() {
        let store = make_store_with_cc14();
        // CC 99 is not in schema — should log trace but not panic
        let msg: &[u8] = &[0xB0, 99, 64];
        handle_message(msg, None, &store); // should not panic
    }
}
