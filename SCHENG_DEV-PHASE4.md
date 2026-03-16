# scheng Control Layer — Phase 4

Four crates completing the control plane.

---

## Overview

| Crate | Role |
|-------|------|
| `scheng-param-store` | Central state: JSON schema → live values → NodeConfig |
| `scheng-input-midi` | MIDI CC → ParamStore (background thread, midir) |
| `scheng-control-osc-wgpu` | OSC UDP → ParamStore (non-blocking, poll each frame) |
| `scheng-hotreload` | File watcher → shader/params live reload |

---

## Add to workspace

```toml
[workspace]
members = [
    # ...
    "crates/scheng-param-store",
    "crates/scheng-input-midi",
    "crates/scheng-control-osc-wgpu",
    "crates/scheng-hotreload",
]
```

---

## params.json schema

All four crates share this JSON format (matches shadecore exactly):

```json
{
  "version": 1,
  "params": [
    {
      "name":        "u_brightness",
      "ty":          "float",
      "min":         0.0,
      "max":         2.0,
      "default":     1.0,
      "smooth":      0.05,
      "midi_cc":     14,
      "midi_channel": 1,
      "osc_addr":    "/scheng/node/proc/uniform/u_brightness",
      "node_label":  "proc",
      "description": "Overall brightness multiplier"
    },
    {
      "name":    "u_tbar",
      "ty":      "float",
      "min":     0.0,
      "max":     1.0,
      "default": 0.5,
      "smooth":  0.02,
      "midi_cc": 7,
      "osc_addr": "/scheng/node/xfad/uniform/u_tbar",
      "node_label": "xfad"
    }
  ]
}
```

---

## Complete instrument wiring

```rust
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

use scheng_graph::{Graph, NodeKind};
use scheng_runtime_wgpu::{WgpuRuntime, FrameCtx};
use scheng_param_store::{ParamStore, NodeConfigBuilder};
use scheng_input_midi::MidiInput;
use scheng_control_osc_wgpu::OscReceiver;
use scheng_hotreload::HotReloader;

fn main() -> anyhow::Result<()> {
    // ── 1. Build graph ────────────────────────────────────────────────────
    let mut graph = Graph::new();
    let src  = graph.add_node(NodeKind::ShaderSource);
    let proc = graph.add_node(NodeKind::ShaderPass);
    let xfad = graph.add_node(NodeKind::Crossfade);
    let out  = graph.add_node(NodeKind::PixelsOut);
    graph.connect_named(src,  "out", proc, "in").unwrap();
    graph.connect_named(proc, "out", xfad, "a").unwrap();
    graph.connect_named(src,  "out", xfad, "b").unwrap();
    graph.connect_named(xfad, "out", out,  "in").unwrap();
    let plan = graph.compile().unwrap();

    // ── 2. GPU runtime ────────────────────────────────────────────────────
    let mut runtime = WgpuRuntime::new(1280, 720)?;

    // ── 3. Parameter store ────────────────────────────────────────────────
    let store = Arc::new(Mutex::new(
        ParamStore::from_json_file("assets/params.json")?
    ));

    // ── 4. Node config builder ────────────────────────────────────────────
    let mut builder = NodeConfigBuilder::new();
    builder.register("src",  src);
    builder.register("proc", proc);
    builder.register("xfad", xfad);
    builder.register("out",  out);

    // Load initial shaders
    builder.set_shader(src,  std::fs::read_to_string("assets/shaders/src.frag")?);
    builder.set_shader(proc, std::fs::read_to_string("assets/shaders/proc.frag")?);
    builder.set_shader(xfad, std::fs::read_to_string("assets/shaders/xfad.frag")?);

    // ── 5. MIDI (background thread) ───────────────────────────────────────
    let midi = MidiInput::connect_first(Arc::clone(&store))
        .map_err(|e| { log::warn!("MIDI not available: {e}"); e })
        .ok(); // Don't fail if no MIDI device

    if let Some(ref m) = midi {
        log::info!("MIDI connected: {}", m.port_name());
    }

    // ── 6. OSC (non-blocking, polled each frame) ──────────────────────────
    let mut osc = OscReceiver::bind("127.0.0.1:9000")
        .map_err(|e| { log::warn!("OSC not available: {e}"); e })
        .ok();

    // ── 7. Hot-reload watcher ─────────────────────────────────────────────
    let mut reloader = HotReloader::new("assets/").ok();
    if let Some(ref mut r) = reloader {
        r.register_shader("assets/shaders/src.frag",  src);
        r.register_shader("assets/shaders/proc.frag", proc);
        r.register_shader("assets/shaders/xfad.frag", xfad);
    }

    // ── 8. Output sink ────────────────────────────────────────────────────
    use scheng_output_ffmpeg::{FfmpegSink, FfmpegConfig, config::OutputTarget};
    let mut sink = FfmpegSink::new(FfmpegConfig {
        width: 1280, height: 720, framerate: 30,
        target: OutputTarget::Rtsp { url: "rtsp://localhost:8554/live".into() },
        ..Default::default()
    })?;

    // ── 9. Render loop ────────────────────────────────────────────────────
    let start = std::time::Instant::now();
    let mut frame: u64 = 0;

    loop {
        // OSC: drain all pending messages
        if let Some(ref mut osc_recv) = osc {
            let mut s = store.lock().unwrap();
            osc_recv.poll(&mut s);
        }

        // Hot-reload: apply any file changes
        if let Some(ref mut r) = reloader {
            let mut s = store.lock().unwrap();
            r.check(&mut builder, &mut s);
        }

        // Advance smoothed parameter values
        store.lock().unwrap().step_frame();

        // Build NodeConfigs from current param values
        let configs = {
            let s = store.lock().unwrap();
            builder.build(&s)
        };

        // Execute one frame
        let ctx = FrameCtx {
            width:  1280,
            height: 720,
            time:   start.elapsed().as_secs_f32(),
            frame,
        };

        runtime.execute_frame(&graph, &plan, &configs, &ctx, &mut sink)?;
        frame += 1;
    }
}
```

---

## OSC addressing

Every param is addressable over OSC. Use the `osc_addr` field in params.json
as the target address. The scheng editor shows the OSC address as a tooltip
on every slider.

```
/scheng/node/proc/uniform/u_brightness  0.75
/scheng/node/xfad/uniform/u_tbar        0.5
```

Short forms also work:
```
/param/u_brightness  0.75
/u_tbar              0.5
```

Send from any OSC client (TouchOSC, Max/MSP, Python python-osc, etc.):
```python
from pythonosc.udp_client import SimpleUDPClient
c = SimpleUDPClient("127.0.0.1", 9000)
c.send_message("/scheng/node/proc/uniform/u_brightness", 0.75)
```

---

## MIDI CC mapping

Map CC numbers in params.json:

```json
{ "name": "u_tbar", "midi_cc": 7 }
```

List available MIDI ports:

```bash
# Quick port listing tool
cargo run --example list_midi_ports -p scheng-input-midi
```

Or in code:
```rust
for port in MidiInput::list_ports().unwrap() {
    println!("  {}", port);
}
```

---

## Hot-reload

Edit a `.frag` file in `assets/shaders/` while the instrument is running.
The watcher detects the change within ~100ms and the new shader compiles
on the next frame. If the shader has a compile error, the old shader
continues running — the error is logged but the instrument doesn't crash.

Edit `assets/params.json` while running to add/remove/adjust params.
Existing values are preserved; new params start at their defaults.

---

## Tests

```bash
# All control layer tests
cargo test -p scheng-param-store -- --nocapture
cargo test -p scheng-input-midi -- --nocapture
cargo test -p scheng-control-osc-wgpu -- --nocapture

# Specific test suites
cargo test -p scheng-param-store schema -- --nocapture
cargo test -p scheng-param-store store  -- --nocapture
cargo test -p scheng-input-midi  cc_message -- --nocapture
```
