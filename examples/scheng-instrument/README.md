# scheng instrument example

A minimal but complete instrument built on the scheng SDK.
Copy this as a starting point for your own instruments.

---

## What it demonstrates

| Feature | How |
|---------|-----|
| Shader loading | `assets/shaders/main.frag` loaded at startup |
| Hot-reload | Edit shader or params.json while running — updates in ~100ms |
| Live params | `assets/params.json` → MIDI CC + OSC → uniform values |
| Syphon output | Metal server `"scheng"` — visible in OBS, VDMX, Resolume |
| FFmpeg output | RTSP stream or local file recording |

---

## Running

```bash
# Default: Syphon output, no FFmpeg
cargo run --release

# Stream to RTSP (start mediamtx first: ./mediamtx)
cargo run --release -- --stream rtsp://localhost:8554/live

# Record to file
cargo run --release -- --record recording.mp4

# Stream + custom resolution
cargo run --release -- --stream rtsp://localhost:8554/live --width 1920 --height 1080

# List MIDI ports
cargo run --release -- --list-midi

# Without Syphon (no framework needed)
cargo run --release --no-default-features --features midi
```

---

## Live control

### MIDI CC (any connected controller)

| CC | Parameter | Default |
|----|-----------|---------|
| 1  | u_speed (mod wheel) | 1.0 |
| 7  | u_brightness (volume) | 1.0 |
| 14 | u_hue_shift | 0.0 |

### OSC (default port 9000)

Send from TouchOSC, Max/MSP, SuperCollider, python-osc, etc.:

```
/scheng/uniform/u_speed       <float 0–5>
/scheng/uniform/u_brightness  <float 0–2>
/scheng/uniform/u_hue_shift   <float -180–180>
```

Python example:
```python
from pythonosc.udp_client import SimpleUDPClient
c = SimpleUDPClient("127.0.0.1", 9000)
c.send_message("/scheng/uniform/u_brightness", 0.75)
```

---

## Hot-reload

Edit `assets/shaders/main.frag` while the instrument is running.
Save the file — the shader recompiles and updates within ~100ms.
If the shader has a syntax error, the previous version continues running
and the error is printed to the console.

Edit `assets/params.json` to add/remove/adjust parameters.
New parameters appear immediately; existing values are preserved.

---

## Extending this instrument

### Add a new parameter

1. Add a uniform to `assets/shaders/main.frag`:
   ```glsl
   uniform float u_zoom;
   ```

2. Add an entry to `assets/params.json`:
   ```json
   {
     "name":      "u_zoom",
     "min":       0.5,
     "max":       2.0,
     "default":   1.0,
     "smooth":    0.05,
     "midi_cc":   15,
     "osc_addr":  "/scheng/uniform/u_zoom"
   }
   ```

That's it. The parameter is now live — MIDI CC 15 and OSC address both work.

### Add a second shader pass

```rust
// In main.rs:
let pass_node = graph.add_node(NodeKind::ShaderPass);
graph.connect_named(main_node, "out", pass_node, "in").unwrap();
graph.connect_named(pass_node, "out", out_node,  "in").unwrap();

builder.register("pass", pass_node);
builder.set_shader(pass_node, std::fs::read_to_string("assets/shaders/pass.frag")?);
```

### Add video input

```rust
use scheng_input_video::VideoSourceManager;

let mut video = VideoSourceManager::new();
video.register(main_node, "assets/clip.mp4", &runtime.ctx.device, &runtime.ctx.queue)?;

// In render loop, before execute_frame:
video.update(ctx.time, &runtime.ctx.queue);
```

### Add Syphon input (receive from another app)

```rust
use scheng_input_syphon::SyphonReceiver;

let mut recv = SyphonReceiver::connect("OBS", mtl_ptr, &device, &queue)?;

// In render loop:
recv.poll_with_device(&device, &queue);
```

---

## Project layout

```
scheng-instrument/
├── Cargo.toml           — dependencies + feature flags
├── assets/
│   ├── params.json      — parameter schema (hot-reloaded)
│   └── shaders/
│       └── main.frag    — main fragment shader (hot-reloaded)
└── src/
    └── main.rs          — instrument entry point
```

---

## SDK crates used

| Crate | Role |
|-------|------|
| `scheng-graph` | Node graph: ShaderSource → PixelsOut |
| `scheng-runtime-wgpu` | GPU execution, wgpu Metal/DX12/Vulkan |
| `scheng-param-store` | JSON schema → live values → NodeConfig |
| `scheng-input-midi` | MIDI CC → params (optional) |
| `scheng-control-osc-wgpu` | OSC UDP → params |
| `scheng-hotreload` | File watcher → shader/params reload |
| `scheng-output-syphon` | Syphon Metal server (macOS, optional) |
| `scheng-output-ffmpeg` | FFmpeg stream/record (optional) |
