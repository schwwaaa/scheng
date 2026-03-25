# scheng-mixer

Two-channel video mixer built on the scheng SDK.

Accepts two Syphon inputs, crossfades between them via MIDI T-bar, outputs via Syphon.

## Directory structure

```
scheng-mixer/
├── Cargo.toml
├── build.rs
├── src/
│   └── main.rs
├── assets/
│   └── shaders/
│       ├── crossfade.frag    ← A/B mix shader (edit live)
│       └── passthrough.frag  ← input pass-through
└── README.md
```

## Setup

Place next to the scheng workspace:
```
projects/
  scheng/
  scheng-mixer/
```

## Run

```bash
# List available Syphon sources first (pass any name)
cargo run --release -- --syphon-a dummy

# Connect to real sources
cargo run --release -- --syphon-a "Resolume Arena" --syphon-b "OBS"

# Custom resolution
cargo run --release -- --syphon-a "Arena" --syphon-b "OBS" --width 1920 --height 1080
```

## MIDI

| CC | Function |
|----|----------|
| CC1 | T-bar (0=A, 127=B) |
| CC7 | Master level |

## Signal chain

```
Syphon "A" → passthrough → ─┐
                              ├→ crossfade → Syphon "scheng-mixer" + preview
Syphon "B" → passthrough → ─┘
```

## Shaders

Edit `assets/shaders/crossfade.frag` live — changes hot-reload instantly.

`u_tbar` — controlled by MIDI CC1
`u_softness` — edge softness (set in code, expose via OSC to control live)
