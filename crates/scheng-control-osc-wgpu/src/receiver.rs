//! `receiver.rs` — non-blocking UDP OSC receiver writing to ParamStore.

use std::net::UdpSocket;

use rosc::{OscMessage, OscPacket, OscType};
use scheng_param_store::ParamStore;

use crate::OscError;

/// Non-blocking OSC UDP receiver.
///
/// Drain all pending messages each frame with `poll()`.
/// Compatible with the scheng-control-osc address convention.
pub struct OscReceiver {
    socket: UdpSocket,
    buf:    Vec<u8>,
}

impl OscReceiver {
    /// Bind to a UDP address (e.g. `"127.0.0.1:9000"`).
    pub fn bind(addr: &str) -> Result<Self, OscError> {
        let socket = UdpSocket::bind(addr)
            .map_err(|e| OscError::Bind { addr: addr.into(), source: e })?;
        socket.set_nonblocking(true)
            .map_err(|e| OscError::Bind { addr: addr.into(), source: e })?;
        log::info!("OSC receiver bound to {}", addr);
        Ok(Self { socket, buf: vec![0u8; 4096] })
    }

    /// Drain all pending OSC messages and apply them to the ParamStore.
    ///
    /// Call once per frame before `store.step_frame()`.
    /// Returns the number of messages processed this frame.
    pub fn poll(&mut self, store: &mut ParamStore) -> usize {
        let mut count = 0;
        loop {
            match self.socket.recv_from(&mut self.buf) {
                Ok((len, _src)) => {
                    if let Ok(packet) = rosc::decoder::decode_udp(&self.buf[..len]) {
                        count += dispatch_packet(&packet.1, store);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    log::warn!("OSC recv error: {}", e);
                    break;
                }
            }
        }
        count
    }

    /// The local address this receiver is bound to.
    pub fn local_addr(&self) -> String {
        self.socket.local_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|_| "unknown".into())
    }
}

// ── Packet dispatch ───────────────────────────────────────────────────────

fn dispatch_packet(packet: &OscPacket, store: &mut ParamStore) -> usize {
    match packet {
        OscPacket::Message(msg)   => dispatch_message(msg, store) as usize,
        OscPacket::Bundle(bundle) => {
            bundle.content.iter()
                .map(|p| dispatch_packet(p, store))
                .sum()
        }
    }
}

fn dispatch_message(msg: &OscMessage, store: &mut ParamStore) -> bool {
    let Some(value) = extract_float(&msg.args) else {
        log::trace!("OSC: no float arg in {}", msg.addr);
        return false;
    };

    let param_name = resolve_addr(&msg.addr);
    log::trace!("OSC: {} → '{}' = {:.4}", msg.addr, param_name, value);

    match store.set_by_osc_addr(&msg.addr, value)
        .or_else(|_| store.set_by_name(param_name, value))
    {
        Ok(())  => true,
        Err(e)  => {
            log::trace!("OSC: unrouted message {} — {}", msg.addr, e);
            false
        }
    }
}

/// Extract the first float-like argument from an OSC message.
/// Coerces Int, Long, and Double to f32 — matches scheng-control-osc behaviour.
fn extract_float(args: &[OscType]) -> Option<f32> {
    args.first().and_then(|a| match a {
        OscType::Float(f)  => Some(*f),
        OscType::Double(d) => Some(*d as f32),
        OscType::Int(i)    => Some(*i as f32),
        OscType::Long(l)   => Some(*l as f32),
        _ => None,
    })
}

/// Resolve an OSC address to a param name.
///
/// Supported forms:
/// - `/scheng/node/<label>/uniform/<name>` → `<name>`
/// - `/param/<name>`                       → `<name>`
/// - `/<name>`                             → `<name>`
fn resolve_addr(addr: &str) -> &str {
    // /scheng/node/<label>/uniform/<name>
    if let Some(rest) = addr.strip_prefix("/scheng/node/") {
        if let Some(pos) = rest.find("/uniform/") {
            return &rest[pos + "/uniform/".len()..];
        }
    }
    // /param/<name>
    if let Some(name) = addr.strip_prefix("/param/") {
        return name;
    }
    // /<name>
    addr.strip_prefix('/').unwrap_or(addr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_full_scheng_addr() {
        assert_eq!(
            resolve_addr("/scheng/node/proc/uniform/u_brightness"),
            "u_brightness"
        );
    }

    #[test]
    fn resolve_param_short_form() {
        assert_eq!(resolve_addr("/param/u_speed"), "u_speed");
    }

    #[test]
    fn resolve_bare_form() {
        assert_eq!(resolve_addr("/u_gain"), "u_gain");
    }

    #[test]
    fn extract_float_variants() {
        assert_eq!(extract_float(&[OscType::Float(0.5)]),  Some(0.5));
        assert_eq!(extract_float(&[OscType::Int(64)]),     Some(64.0));
        assert_eq!(extract_float(&[OscType::Double(1.0)]), Some(1.0));
        assert_eq!(extract_float(&[OscType::String("x".into())]), None);
        assert_eq!(extract_float(&[]), None);
    }
}
