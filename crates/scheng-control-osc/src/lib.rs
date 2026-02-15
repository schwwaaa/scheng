//! scheng-control-osc
//!
//! Minimal OSC control-plane helper used by scheng examples.
//!
//! This crate intentionally stays tiny: it only knows how to receive OSC packets
//! over UDP and extract simple (path, f32) parameter updates.
//!
//! rosc 0.10.x API note:
//! - `rosc::decoder::decode_udp` returns `Result<(&[u8], OscPacket), _>` (nom-style),
//!   where the first tuple element is the *unconsumed remainder* of the buffer.

use std::io;
use std::net::UdpSocket;

use rosc::{OscPacket, OscType};

/// Non-blocking UDP OSC receiver that extracts parameter messages.
///
/// Convention:
/// - Address: "/param/<name>" or "/<name>"
/// - Value: first argument, coercible to f32 (Float, Double, Int, Long)
#[derive(Debug)]
pub struct OscParamReceiver {
    sock: UdpSocket,
    buf: [u8; 2048],
}

impl OscParamReceiver {
    /// Bind to an address like "127.0.0.1:9000" and put the socket in non-blocking mode.
    pub fn bind(addr: &str) -> io::Result<Self> {
        let sock = UdpSocket::bind(addr)?;
        sock.set_nonblocking(true)?;
        Ok(Self {
            sock,
            buf: [0u8; 2048],
        })
    }

    /// Poll the socket and return all parameter updates available right now.
    ///
    /// This never blocks; it drains the UDP socket until `WouldBlock`.
    pub fn poll(&mut self) -> Vec<(String, f32)> {
        let mut out: Vec<(String, f32)> = Vec::new();

        loop {
            match self.sock.recv_from(&mut self.buf) {
                Ok((n, _from)) => {
                    // decode_udp is nom-style: Ok((rest, packet))
                    if let Ok((_rest, pkt)) = rosc::decoder::decode_udp(&self.buf[..n]) {
                        extract_from_packet(pkt, &mut out);
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_e) => break, // ignore transient socket errors for now
            }
        }

        out
    }
}

/// Walk a packet/bundle tree and push parsed param messages into `out`.
fn extract_from_packet(pkt: OscPacket, out: &mut Vec<(String, f32)>) {
    match pkt {
        OscPacket::Message(m) => {
            if let Some(kv) = parse_param_message(&m.addr, &m.args) {
                out.push(kv);
            }
        }
        OscPacket::Bundle(b) => {
            for p in b.content {
                extract_from_packet(p, out);
            }
        }
    }
}

/// Parse a message into a `(name, value)` pair if it matches our convention.
fn parse_param_message(addr: &str, args: &[OscType]) -> Option<(String, f32)> {
    let name = addr.strip_prefix("/param/").or_else(|| addr.strip_prefix('/'))?;
    let v0 = args.first()?;
    let v = match *v0 {
        OscType::Float(x) => x,
        OscType::Double(x) => x as f32,
        OscType::Int(x) => x as f32,
        OscType::Long(x) => x as f32,
        _ => return None,
    };
    Some((name.to_string(), v))
}
