/// OSC input — maps incoming UDP OSC messages to engine parameters live.
///
/// Address schema:
///   /scheng/node/{id}/mix          f32          set_mix on Crossfade node
///   /scheng/node/{id}/weights      f32 f32 f32 f32   set_weights on MatrixMix4
///   /scheng/node/{id}/shader       string       set_shader then auto-compile
///   /scheng/compile                             trigger compile
///
/// Port: SCHENG_OSC_PORT env var, default 57120.
/// Broadcasts parameter changes to all WebSocket clients so the editor stays in sync.

use crate::state::BridgeState;
use crate::ws::{compile_bundle, SharedBundle};
use rosc::{OscMessage, OscPacket, OscType};
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

pub fn run_osc(
    ws_state: Arc<Mutex<BridgeState>>,
    bundle: SharedBundle,
    bcast_tx: broadcast::Sender<String>,
) {
    let port: u16 = std::env::var("SCHENG_OSC_PORT")
        .ok().and_then(|s| s.parse().ok()).unwrap_or(57120);
    let addr = format!("0.0.0.0:{port}");
    let sock = match UdpSocket::bind(&addr) {
        Ok(s) => { eprintln!("[osc] listening on udp:{port}"); s }
        Err(e) => { eprintln!("[osc] bind failed ({addr}): {e}"); return; }
    };
    let mut buf = [0u8; 65535];
    loop {
        let (n, _) = match sock.recv_from(&mut buf) {
            Ok(r) => r,
            Err(e) => { eprintln!("[osc] recv: {e}"); continue; }
        };
        match rosc::decoder::decode_udp(&buf[..n]) {
            Ok((_, pkt)) => handle_packet(pkt, &ws_state, &bundle, &bcast_tx),
            Err(e) => eprintln!("[osc] decode: {e}"),
        }
    }
}

fn handle_packet(pkt: OscPacket, ws: &Arc<Mutex<BridgeState>>, bundle: &SharedBundle, tx: &broadcast::Sender<String>) {
    match pkt {
        OscPacket::Message(m) => handle_msg(m, ws, bundle, tx),
        OscPacket::Bundle(b)  => b.content.into_iter().for_each(|p| handle_packet(p, ws, bundle, tx)),
    }
}

fn handle_msg(msg: OscMessage, ws: &Arc<Mutex<BridgeState>>, bundle: &SharedBundle, tx: &broadcast::Sender<String>) {
    let addr = msg.addr.clone();
    eprintln!("[osc] {addr} {:?}", msg.args);

    // /scheng/compile
    if addr == "/scheng/compile" {
        let s = ws.lock().unwrap();
        match compile_bundle(&s) {
            Ok(b) => {
                let (nc, ec) = (b.plan.nodes.len(), b.plan.edges.len());
                *bundle.lock().unwrap() = Some(b);
                let _ = tx.send(serde_json::json!({"event":"compiled","node_count":nc,"edge_count":ec}).to_string());
                eprintln!("[osc] compiled: {nc} nodes {ec} edges");
            }
            Err(e) => eprintln!("[osc] compile error: {e}"),
        }
        return;
    }

    // /scheng/node/{id}/{param}
    let parts: Vec<&str> = addr.trim_start_matches('/').split('/').collect();
    if parts.len() < 4 || parts[0] != "scheng" || parts[1] != "node" { return; }
    let node_id = parts[2];
    let param   = parts[3];
    let mut s   = ws.lock().unwrap();

    match param {
        "mix" => {
            if let Some(OscType::Float(v)) = msg.args.first() {
                let v = v.clamp(0.0, 1.0);
                if let Ok(()) = s.set_mix(node_id, v) {
                    let _ = tx.send(serde_json::json!({"event":"param_updated","node_id":node_id,"param":"mix","value":v}).to_string());
                }
            }
        }
        "weights" => {
            let fs: Vec<f32> = msg.args.iter().filter_map(|a| if let OscType::Float(f) = a { Some(*f) } else { None }).collect();
            if fs.len() >= 4 {
                let w = [fs[0], fs[1], fs[2], fs[3]];
                if let Ok(()) = s.set_weights(node_id, w) {
                    let _ = tx.send(serde_json::json!({"event":"weights_updated","node_id":node_id,"weights":w}).to_string());
                }
            }
        }
        "shader" => {
            if let Some(OscType::String(frag)) = msg.args.into_iter().next() {
                if let Ok(()) = s.set_shader(node_id, None, frag) {
                    let _ = tx.send(serde_json::json!({"event":"shader_updated","node_id":node_id}).to_string());
                    drop(s);
                    let s2 = ws.lock().unwrap();
                    if let Ok(b) = compile_bundle(&s2) {
                        let (nc, ec) = (b.plan.nodes.len(), b.plan.edges.len());
                        *bundle.lock().unwrap() = Some(b);
                        let _ = tx.send(serde_json::json!({"event":"compiled","node_count":nc,"edge_count":ec}).to_string());
                    }
                }
            }
        }
        // /scheng/node/{id}/uniform/{name}  f32
        name if name.starts_with("uniform/") => {
            let uname = name.trim_start_matches("uniform/").to_string();
            if let Some(OscType::Float(v)) = msg.args.first() {
                if let Ok(()) = s.set_uniform(node_id, uname.clone(), *v) {
                    let _ = tx.send(serde_json::json!({"event":"uniform_updated","node_id":node_id,"name":uname,"value":v}).to_string());
                }
            }
        }
        other => eprintln!("[osc] unknown param '{other}' on node '{node_id}'"),
    }
}
