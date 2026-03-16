# scheng-tauri — Setup & Build Guide

## Prerequisites

### All platforms
```bash
# Rust stable
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Tauri CLI v2
cargo install tauri-cli --version "^2"
```

### macOS
```bash
xcode-select --install
```

### Windows
- Visual Studio Build Tools (C++ workload)
- WebView2 (bundled with Windows 11, install separately on Windows 10)

### Linux
```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
```

---

## Add to workspace

In root `Cargo.toml`:

```toml
[workspace]
members = [
    # ... existing crates ...
    "crates/scheng-tauri",
]
```

---

## Directory layout

```
crates/scheng-tauri/
├── Cargo.toml
├── build.rs
├── tauri.conf.json
├── src/
│   ├── main.rs         # binary entry point (just calls run())
│   ├── lib.rs          # Tauri builder + setup
│   ├── commands.rs     # #[tauri::command] IPC handlers
│   ├── engine.rs       # AppState shared between threads
│   ├── render_loop.rs  # render thread (owns WgpuRuntime)
│   └── preview.rs      # pixel readback → JPEG → base64
├── icons/              # app icons (see below)
└── ui/
    └── index.html      # instrument UI (pure HTML/CSS/JS, no bundler needed)
```

---

## Icons

Tauri requires app icons. Generate them from a 1024×1024 PNG:

```bash
# Using the Tauri icon generator
cargo tauri icon path/to/your/icon.png
# Places icons in src-tauri/icons/ — move to crates/scheng-tauri/icons/
```

Or create a placeholder for development:
```bash
mkdir -p crates/scheng-tauri/icons
# Copy icons from an existing Tauri template, or create minimal placeholders
```

---

## Development run

```bash
# From the workspace root:
cargo tauri dev --manifest-path crates/scheng-tauri/Cargo.toml

# Or cd into the crate:
cd crates/scheng-tauri
cargo tauri dev
```

This opens a native window with the WebView UI and the scheng render running live.

---

## Production build

```bash
# macOS .app
cargo tauri build --manifest-path crates/scheng-tauri/Cargo.toml

# macOS universal (Intel + Apple Silicon)
cargo tauri build --target universal-apple-darwin

# Windows .exe + NSIS installer
# (run on Windows or cross-compile)
cargo tauri build

# Linux .deb + AppImage
cargo tauri build
```

Output: `target/release/bundle/`

---

## Syphon output (macOS)

Enable the syphon feature and place `Syphon.framework` in `vendor/`:

```toml
# In your instrument's Cargo.toml or via feature flag:
scheng-tauri = { path = "crates/scheng-tauri", features = ["syphon"] }
```

```bash
cargo tauri build --features syphon
```

---

## Assets directory

The render loop looks for `assets/` relative to the working directory.
For development, run from the workspace root or set the working directory:

```
assets/
├── params.json            ← parameter schema (hot-reloaded)
├── shaders/
│   └── default.frag       ← main fragment shader (hot-reloaded)
└── output.json            ← optional output config
```

Minimal `assets/params.json`:
```json
{
  "version": 1,
  "params": [
    {
      "name": "u_speed",
      "min": 0.0,
      "max": 5.0,
      "default": 1.0,
      "smooth": 0.05,
      "midi_cc": 1
    }
  ]
}
```

Minimal `assets/shaders/default.frag`:
```glsl
#version 330 core
in vec2 v_uv;
out vec4 fragColor;
uniform float uTime;
uniform vec2 uResolution;

void main() {
    float r = v_uv.x + 0.5 * sin(uTime);
    float g = v_uv.y + 0.5 * cos(uTime * 0.7);
    fragColor = vec4(r, g, 0.2, 1.0);
}
```

---

## IPC Reference (JavaScript → Rust)

```typescript
import { invoke } from '@tauri-apps/api/core';

// Get parameter schema for building UI
const schema = await invoke('get_params');
// → { version: 1, params: [{name, min, max, default, midi_cc, ...}] }

// Set a parameter value (from slider, MIDI, etc.)
await invoke('set_param', { name: 'u_speed', value: 1.5 });

// Switch output mode
await invoke('set_output_mode', { mode: 'preview' });   // no external output
await invoke('set_output_mode', { mode: 'syphon' });    // macOS Metal sharing
await invoke('set_output_mode', { mode: 'stream' });    // RTSP/RTMP
await invoke('set_output_mode', { mode: 'record' });    // local file

// Recording
await invoke('start_recording', { path: 'output.mp4' });
await invoke('stop_recording');

// Engine status
const status = await invoke('get_engine_status');
// → { running, frame, output_mode, is_recording, adapter_name }

// Load a graph patch (Phase 7)
await invoke('load_graph_json', { json: JSON.stringify(patch) });
```

## Events (Rust → JavaScript)

```typescript
import { listen } from '@tauri-apps/api/event';

// Preview frame at ~15fps
await listen('preview-frame', (event) => {
    const img = document.getElementById('preview');
    img.src = 'data:image/jpeg;base64,' + event.payload;
});

// params.json was hot-reloaded
await listen('params-reloaded', () => {
    rebuildParamUI();
});
```

---

## Testing

```bash
# Unit tests (no GPU needed for most)
cargo test -p scheng-tauri -- --nocapture

# Check compilation
cargo check -p scheng-tauri
```
