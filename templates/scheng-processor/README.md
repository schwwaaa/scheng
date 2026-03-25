# scheng-processor

Webcam input → proc-amp shader → preview + Syphon output.

A live video effect unit built on the scheng SDK. Applies classic analog
processing amplifier controls to a webcam feed in real-time via MIDI.

## Directory structure

```
scheng-processor/
├── Cargo.toml
├── build.rs
├── src/
│   └── main.rs
├── assets/
│   └── shaders/
│       └── proc_amp.frag    ← edit live, hot-reloads instantly
└── README.md
```

## Run

```bash
# Default camera
cargo run --release

# List available cameras
cargo run --release -- --list-cameras

# Use camera index 1
cargo run --release -- --webcam 1

# 1080p
cargo run --release -- --width 1920 --height 1080
```

## MIDI controls

| CC | Parameter    | Range          | Default |
|----|-------------|----------------|---------|
| CC1 | Brightness | -1.0 → +1.0   | 0.0     |
| CC2 | Contrast   | 0.0 → 3.0     | 1.0     |
| CC3 | Saturation | 0.0 → 3.0     | 1.0     |
| CC4 | Hue        | -180° → +180° | 0.0     |

## Signal chain

```
Webcam
  ↓
ShaderSource (passthrough)
  ↓
ShaderPass (proc_amp.frag)  ← MIDI CC1-4 control uniforms
  ↓
PixelsOut → preview window + Syphon "scheng-processor"
```

## Adding effects

Edit `assets/shaders/proc_amp.frag` live — the shader hot-reloads on save.
Add new `uniform float u_*` variables and they're automatically exposed via
MIDI and OSC.
