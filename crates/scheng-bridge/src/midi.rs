/// MIDI input — maps CC messages to engine parameters.
///
/// Mapping file: scheng-midi.json (next to binary, or SCHENG_MIDI_MAP env var)
/// If no file found: connects to first MIDI device and logs all CC values
/// so you can see exactly what your controller sends.
///
/// scheng-midi.json format:
/// {
///   "device": "MPD218",   <- partial match, omit for first device
///   "mappings": [
///     { "ch": 1, "cc": 7,  "node": "mix_0",  "param": "mix"     },
///     { "ch": 1, "cc": 14, "node": "pass_1", "param": "mix"     },
///     { "ch": 1, "cc": 74, "node": "mix4_0", "param": "weight0" }
///   ]
/// }
///
/// CC 0–127 → 0.0–1.0.
/// param values: "mix", "weight0", "weight1", "weight2", "weight3"

use crate::state::BridgeState;
use crate::ws::SharedBundle;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[derive(Debug, serde::Deserialize)]
struct Mapping { ch: u8, cc: u8, node: String, param: String }

#[derive(Debug, serde::Deserialize)]
struct MidiConfig { device: Option<String>, mappings: Vec<Mapping> }

pub fn run_midi(
    ws_state: Arc<Mutex<BridgeState>>,
    _bundle: SharedBundle,
    bcast_tx: broadcast::Sender<String>,
) {
    let map_path = std::env::var("SCHENG_MIDI_MAP").unwrap_or_else(|_| "scheng-midi.json".into());
    let config: Option<MidiConfig> = std::fs::read_to_string(&map_path)
        .ok().and_then(|s| serde_json::from_str(&s).ok());
    match &config {
        Some(c) => eprintln!("[midi] {} mappings from {map_path}", c.mappings.len()),
        None    => eprintln!("[midi] no mapping at {map_path} — logging all CC"),
    }

    let midi = match midir::MidiInput::new("scheng-bridge") {
        Ok(m) => m,
        Err(e) => { eprintln!("[midi] init: {e}"); return; }
    };
    let ports = midi.ports();
    if ports.is_empty() { eprintln!("[midi] no MIDI devices found"); return; }

    let port = config.as_ref()
        .and_then(|c| c.device.as_ref())
        .and_then(|name| ports.iter().find(|p| midi.port_name(p).unwrap_or_default().contains(name.as_str())))
        .unwrap_or(&ports[0]);

    let dev_name = midi.port_name(port).unwrap_or_else(|_| "unknown".into());
    eprintln!("[midi] connecting: {dev_name}");

    let config = Arc::new(config);

    let _conn = midi.connect(port, "scheng-midi", move |_ts, msg, _| {
        if msg.len() < 3 { return; }
        let kind = msg[0] & 0xF0;
        let ch   = (msg[0] & 0x0F) + 1;
        if kind == 0xB0 {
            let cc  = msg[1];
            let val = msg[2] as f32 / 127.0;
            eprintln!("[midi] CC ch={ch} cc={cc} val={:.3}", val);
            if let Some(ref cfg) = *config {
                for m in cfg.mappings.iter().filter(|m| m.ch == ch && m.cc == cc) {
                    apply(&m.node, &m.param, val, &ws_state, &bcast_tx);
                }
            }
        }
    }, ());

    match _conn {
        Ok(_c) => { eprintln!("[midi] connected: {dev_name}"); loop { std::thread::sleep(std::time::Duration::from_secs(1)); } }
        Err(e) => eprintln!("[midi] connect: {e}"),
    }
}

fn apply(node_id: &str, param: &str, value: f32, ws: &Arc<Mutex<BridgeState>>, tx: &broadcast::Sender<String>) {
    let mut s = ws.lock().unwrap();
    match param {
        "mix" => {
            if let Ok(()) = s.set_mix(node_id, value) {
                let _ = tx.send(serde_json::json!({"event":"param_updated","node_id":node_id,"param":"mix","value":value}).to_string());
            }
        }
        w @ ("weight0"|"weight1"|"weight2"|"weight3") => {
            let idx = w.trim_start_matches("weight").parse::<usize>().unwrap_or(0);
            if let Some(meta) = s.nodes.get(node_id) {
                let bid = meta.bridge_id.clone();
                let mut weights = s.shaders.matrix.get(&bid)
                    .map(|m| m.weights).unwrap_or([0.25; 4]);
                weights[idx] = value;
                if let Ok(()) = s.set_weights(node_id, weights) {
                    let _ = tx.send(serde_json::json!({"event":"weights_updated","node_id":node_id,"weights":weights}).to_string());
                }
            }
        }
        other => eprintln!("[midi] unknown param '{other}'"),
    }
}
