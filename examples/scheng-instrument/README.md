# scheng-instrument

The reference instrument for the scheng SDK. If you're new to scheng, start here.

This is a full-featured instrument that demonstrates every SDK capability in one place — hot-reload shaders, MIDI, OSC, webcam, video, Syphon, NDI, RTMP, and file recording.

---

## Getting started in 3 steps

### 1. Clone the SDK

```bash
git clone https://github.com/yourusername/scheng
cd scheng
```

### 2. Run the example instrument

```bash
cd examples/scheng-instrument
cargo run --release
```

A window opens showing an animated gradient shader. That's it — you're rendering on the GPU.

### 3. Edit the shader live

Open `assets/shaders/main.frag` in any editor. Change something — save. The window updates instantly. No restart, no recompile wait. This is hot-reload.

---

## What's demonstrated

| Feature | How to activate |
|---------|----------------|
| Hot-reload GLSL | Edit any file in `assets/shaders/` and save |
| MIDI control | Connect any MIDI device — CC1 maps to `u_brightness` |
| OSC control | Send to `127.0.0.1:9000` — address `/scheng/u_brightness` |
| Webcam input | `--webcam 0` (use `--list-cameras` to find your index) |
| Video file | `--video path/to/clip.mp4` |
| Syphon receive | `--syphon-receive "OBS"` (macOS) |
| RTMP receive | `--rtmp-in rtmp://localhost:1935/live/key` |
| Syphon output | Always active on macOS as `"scheng-instrument"` |
| NDI output | Always active when NDI SDK is installed |
| RTMP stream | `--stream rtmp://localhost:1935/live/key` |
| RTSP stream | `--stream rtsp://localhost:8554/live` |
| File recording | `--record output.mp4` |
| 4K | `--width 3840 --height 2160` |
| MSAA | `--msaa 4` |

---

## CLI reference

```bash
# Resolution
cargo run --release -- --width 1920 --height 1080
cargo run --release -- --width 3840 --height 2160 --msaa 4

# Inputs
cargo run --release -- --webcam 1
cargo run --release -- --list-cameras
cargo run --release -- --video clip.mp4
cargo run --release -- --syphon-receive "Resolume Arena"
cargo run --release -- --rtmp-in rtmp://localhost:1935/live/key

# Outputs
cargo run --release -- --stream rtmp://localhost:1935/live/key
cargo run --release -- --stream rtsp://localhost:8554/live
cargo run --release -- --record output.mp4
```

---

## Project layout

```
examples/scheng-instrument/
├── Cargo.toml
├── build.rs             ← rpath for Syphon.framework (macOS)
├── src/
│   └── main.rs          ← instrument entry point
└── assets/
    └── shaders/
        └── main.frag    ← edit this live
```

---

## Building your own instrument

Once you understand how this example works, use a template as your starting point:

- **`scheng-gradient`** — smallest possible instrument, same hot-reload pattern
- **`scheng-mixer`** — adds Syphon inputs and MIDI T-bar
- **`scheng-processor`** — adds webcam and per-pixel effects
- **`scheng-video-mixer`** — adds video file playback

See the [Developer Reference](https://yourusername.github.io/scheng/developer-reference.html) for full SDK documentation.

---

## Troubleshooting

**No MIDI ports found** — Enable IAC Driver: Audio MIDI Setup → MIDI Studio → IAC Driver → Device is online.

**Webcam fails to open** — Run `--list-cameras` and use the correct index. FaceTime HD is typically index 1 on macOS.

**Syphon sources not found** — Sources appear at frame 5+. If nothing shows after a few seconds, verify the sending app has Syphon output enabled.

**Shader compile error** — Check the log output for the GLSL error. Line numbers are relative to your shader body.
