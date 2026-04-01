# scheng SDK — New Session Prompt
# Paste this at the start of every new Claude session.

## Project
scheng = Rust SDK for GPU-accelerated video synthesis instruments.
Version: v0.1.0 stable.
Location: `/Users/tgm/Documents/SPLASH/scheng/` (SDK workspace)

## All instruments — locations on disk
```
/Users/tgm/Documents/SPLASH/
  scheng/                  ← SDK workspace (DO NOT MODIFY without reading source)
  scheng-gradient/         ← stabilized — minimal hot-reload starter
  scheng-mixer/            ← stabilized — Syphon A/B crossfade, MIDI T-bar
  scheng-processor/        ← stabilized — webcam + solarize effect
  scheng-video-mixer/      ← stabilized — two video files crossfade
  scheng-raymarcher/       ← stabilized — 3D raymarcher, FPS overlay, Y-flip fixed
  scheng-playground/       ← stabilized — 8 shaders, all CC1-8 wired, cycle with arrows
  scheng-feedback/         ← stabilized — motion blur raymarcher, CC1-8, temporal trails
  scheng-sdf/              ← stabilized — 6 SDF scenes, key mode (CC7), cycle with arrows
```

## Completed work (sessions 1–7)
- All core I/O primitives: Syphon in/out, NDI in/out, webcam, video file, RTMP/RTSP, MIDI, OSC
- Graph system: ShaderSource, ShaderPass, Crossfade, PreviousFrame, PixelsOut
- ParamStore: MIDI CC routing, smoothing, OSC address routing
- Hot-reload: AssetWatcher, shader recompile on save
- bt.709 colorspace output on all FFmpeg paths
- MSAA: sample_count in FrameCtx
- Render scale: --render-scale flag on all templates
- Performance guide: `/mnt/user-data/outputs/scheng-performance-guide.docx`
- Web docs: `/mnt/user-data/outputs/scheng-web/` (index.html, architecture.html, sdk-reference.html, sdk-diagrams.html)
- Plugin contract: `/mnt/user-data/outputs/docs-package/plugin.rs` + PLUGIN-CONTRACT.md
- CONTRIBUTING.md, CHANGELOG.md, LICENSE, README templates for all instruments
- scheng-sdf: 6 SDF scenes, CC7=0 key mode (black bg for Resolume), CC7>0 abstract mode

## Critical proven patterns

### rpath — ALWAYS use .cargo/config.toml (build.rs is unreliable for this)
```bash
mkdir -p /Users/tgm/Documents/SPLASH/[project]/.cargo
cat > /Users/tgm/Documents/SPLASH/[project]/.cargo/config.toml << 'EOF'
[target.aarch64-apple-darwin]
rustflags = ["-C", "link-arg=-Wl,-rpath,/Users/tgm/Documents/SPLASH/scheng/vendor"]
[target.x86_64-apple-darwin]
rustflags = ["-C", "link-arg=-Wl,-rpath,/Users/tgm/Documents/SPLASH/scheng/vendor"]
EOF
cargo clean && cargo run --release
```

### PreviousFrame node — CONFIRMED from SDK source
- `NodeClass::Source` — has ONLY port `"out"`. NO `"in"` port.
- Connect: `g.connect_named(node_prev, "out", node_fb, "in")`
- Runtime captures the frame internally. You read from it, never write to it.
- Additive compositing approach works conceptually but visual result was poor.
- Motion blur in shader (scheng-feedback) is the preferred approach until this is revisited.

### Port names (confirmed from scheng-graph/src/protocol.rs and lib.rs)
- ShaderSource:   ports `"out"`
- ShaderPass:     ports `"in"`, `"out"`
- Crossfade:      ports `"a"`, `"b"`, `"out"`
- PreviousFrame:  port  `"out"` ONLY (Source class)
- PixelsOut:      port  `"in"` ONLY

### Y-flip rule (always required for 3D / UV sampling)
- scene.frag UV: `vec2(v_uv.x, 1.0 - v_uv.y)`
- Overlay sampling iChannel0: `texture(iChannel0, vec2(v_uv.x, 1.0 - v_uv.y))`
- Overlay pixel coords: `vec2(v_uv.x * uResolution.x, (1.0 - v_uv.y) * uResolution.y)`

### Render scale (in all templates)
```rust
fn render_size(w: u32, h: u32, scale: f32) -> (u32, u32) {
    let rw = ((w as f32 * scale) as u32).max(64);
    let rh = ((h as f32 * scale) as u32).max(36);
    ((rw + 1) & !1, (rh + 1) & !1)
}
// In tick(): let (rw, rh) = render_size(self.args.width, self.args.height, self.args.render_scale);
// FrameCtx uses rw/rh; surface stays at display dimensions
```

### graph.compile() — call inline per frame, do not store typed plan
```rust
let plan = match self.graph.compile() {
    Ok(p) => p, Err(e) => { log::error!("Graph: {e}"); return; }
};
```

### Borrow checker — extract ALL values BEFORE mutable borrows
```rust
let shader_src = self.shader_src.clone();
let uniforms = { let mut s = self.param_store.lock().unwrap(); s.step_frame(); s.all_values().clone() };
let time = self.start.elapsed().as_secs_f32();
let (w, h, msaa) = (self.args.width, self.args.height, self.args.msaa);
// THEN: let (Some(ref mut runtime), Some(ref mut preview)) = ...
```

### Mutex deadlock — NEVER lock the same Mutex twice in one expression
```rust
// ❌ DEADLOCK — two MutexGuards alive simultaneously on same thread
log::info!("{} {}", store.lock().unwrap().get("x"), store.lock().unwrap().get("y"));

// ✅ CORRECT — extract before use
let x = store.lock().unwrap().get("x").unwrap_or(0.0);
let y = store.lock().unwrap().get("y").unwrap_or(0.0);
log::info!("{x} {y}");
```

### Smooth SDF edges (analytically AA — better than MSAA, free)
```glsl
float px   = 2.0 / min(uResolution.x, uResolution.y);
float fill = 1.0 - smoothstep(-px, px, d);   // d = SDF value
float glow = exp(-max(d, 0.0) * 12.0) * 0.4;
```

### GLSL naga compatibility rules
- Never define functions that are unused (naga may reject dead code)
- Don't access uniforms from inside helper functions — pass as parameters
- `atan(y, x)` two-arg form IS supported in recent naga
- `mod()` works with floats, be careful with int math
- No GLSL 4.x features — stay in GLSL 3.30 range

### step_frame() must be called every tick
```rust
let mut s = self.param_store.lock().unwrap();
s.step_frame();  // REQUIRED — advances smoother, MIDI values won't update without this
```

### Shader uniform declarations must be explicit
- The compat layer injects standard uniforms (uTime, uResolution, v_uv, fragColor, iChannel0-3)
- Custom uniforms (u_p1–u_p8, etc.) MUST have actual `uniform float u_p1;` declarations
- Comments alone are not declarations — naga will error with UnknownVariable

## Shader path loading — use executable-relative resolution
```rust
fn shader_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let c = exe.parent().unwrap_or(std::path::Path::new("."))
            .join("assets").join("shaders");
        if c.is_dir() { return c; }
    }
    PathBuf::from("assets/shaders")
}
```

## Performance reference
| --render-scale | Speed gain | Use case |
|---|---|---|
| 1.0 | 1× | Native, simple 2D shaders |
| 0.75 | ~1.8× | Complex 2D at 4K |
| 0.5 | ~4× | Raymarcher at 1080p/4K |
| 0.33 | ~9× | Raymarcher at 4K, Raspberry Pi 5 |

| Hardware | 1080p raymarcher | 4K raymarcher |
|---|---|---|
| M1 Mac Mini | ~45–60 fps native | render-scale 0.5 |
| M2 Mac Mini | ~70–90 fps native | render-scale 0.5–0.75 |
| M2 Pro | ~120+ fps native | render-scale 0.75 |
| Raspberry Pi 5 | ~15–25 fps | Not viable |

## Known gotchas
| Symptom | Cause | Fix |
|---|---|---|
| dyld: Syphon not loaded | rpath missing | .cargo/config.toml (build.rs unreliable) |
| "to port not found" | Wrong port name | PreviousFrame has only "out" port |
| Scene upside down | Missing Y-flip | `vec2(v_uv.x, 1.0 - v_uv.y)` |
| MIDI no effect | step_frame() not called | Call every tick before reading values |
| Borrow checker E0502 | Immutable borrow after mutable | Extract all values before mutable borrows |
| UnknownVariable u_p1 | Uniform in comment only | Add actual `uniform float u_p1;` declaration |
| Freeze/deadlock | Double Mutex lock in same expr | Extract value to variable first |
| Crash on shader switch | Unused function in GLSL | Remove dead code — naga rejects it |
| Keys not working | Shader dir not found | Run from project root via cargo run |

## Punchlist — remaining work
### Near-term (in progress)
- [ ] SBC targets — Raspberry Pi 5 + NVIDIA Jetson Orin documentation and settings
- [ ] crates.io publishing — workspace ready, need version pinning + README polish
- [ ] scheng-feedback v4 — PreviousFrame additive compositing, better foreground design

### SDK completion before marketing push
- [ ] All templates documented in README with screenshots
- [ ] SBC deployment story complete
- [ ] crates.io published
- [ ] Plugin ecosystem — third-party instrument template

### Medium-term
- [ ] Spout output (Windows) — C++ bridge
- [ ] TAA — depends on PreviousFrame working well
- [ ] 3D vertex pipeline — mesh rendering
- [ ] wgpu 24 upgrade

### Long-term / marketing
- [ ] University/education toolkit
- [ ] Video documentation / demos
- [ ] Community examples repository
- [ ] Conference talks / showreels

## Session transcript index
See `/mnt/transcripts/journal.txt` for all session transcript locations.
