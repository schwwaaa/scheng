# scheng-feedback

<p align="center">
  <img src="https://img.shields.io/badge/scheng-template-3b82f6?style=flat-square"/>
  <img src="https://img.shields.io/badge/technique-luma%20key-8b5cf6?style=flat-square"/>
  <img src="https://img.shields.io/badge/node-PreviousFrame-06b6d4?style=flat-square"/>
  <img src="https://img.shields.io/badge/MIDI-CC1--CC8-f59e0b?style=flat-square"/>
</p>

Temporal feedback with luma key compositing. The rendered output from each frame feeds back into the next — creating trails, smear, echo, and accumulation effects. A luma key gates what cuts through the feedback versus what persists and decays.

---

## How it works

The key concept: **brightness controls depth in time.**

- **Bright areas** of the generator shader key through the feedback buffer — they appear crisp, replace what was there before, and start new trails.
- **Dark areas** let the feedback trail accumulate — old frames persist, decay slowly, and drift in colour.

Every frame:
```
new_frame = mix(feedback × decay, generator, luma_mask)
```

The result is then fed back as the input to the next frame — creating an infinite echo loop where old frames slowly fade and shift while new content continuously cuts through.

---

## Graph

```
ShaderSource (generator.frag)
      │ iChannel0 (current generator frame)
      ▼
ShaderPass  (feedback.frag) ◄── iChannel1 ── PreviousFrame ◄─┐
      │                                                        │
      │ (feedback output captured here each frame) ──────────►┘
      │
      ▼
PixelsOut → Preview + Syphon
```

`PreviousFrame` is a special scheng node that reads the **previous frame's** output of whatever node feeds its input port. This breaks what would otherwise be a cycle in the graph — enabling temporal feedback without a DAG violation.

---

## Run

```bash
# Move to SPLASH/ first
cd /Users/tgm/Documents/SPLASH/scheng-feedback
cargo run --release

# Higher resolution
cargo run --release -- --width 1920 --height 1080
```

---

## MIDI controls

| CC | Parameter | Effect |
|----|-----------|--------|
| CC1 | Generator speed | How fast shapes animate |
| CC2 | Generator scale | Size of orbiting shapes |
| CC3 | Complexity | Number of shapes (2–6) |
| CC4 | Generator hue | Starting colour of shapes |
| CC5 | Feedback decay | Trail length — higher = longer trails |
| CC6 | Zoom | Slow inward/outward zoom creates vortex or infinite-zoom effect |
| CC7 | Rotation | Clockwise/counter-clockwise drift per frame creates spirals |
| CC8 | Hue drift | Trails colour-shift over time — full right = rainbow cycling |

**Suggested starting points:**
- Long trails, slow spiral: CC5=100, CC7=70, CC8=55
- Fast accumulation / smear: CC5=90, CC6=65, CC1=80
- Pulsing echo: CC5=95, CC3=127, CC4 sweep slowly

---

## Luma key settings

The threshold controlling what brightness "cuts through" the feedback is set as constants in `src/main.rs`:

```rust
const DEFAULT_LUMA_THRESH: f32 = 0.25; // 0–1: brightness level that keys through
const DEFAULT_LUMA_SOFT:   f32 = 0.35; // 0–1: edge softness (0=hard, 1=gradual)
```

**Threshold guide:**
- Low (0.05–0.15) — almost all content cuts through, short trails, fast absorption
- Medium (0.2–0.4) — only shape fills cut through, edges create soft trails
- High (0.5–0.8) — only the brightest core cuts through, very long delicate trails

To make these MIDI-controllable, add `ParamDef` entries in `build_param_store()` for `u_luma_thresh` and `u_luma_soft`.

---

## Hot-reload

Edit either shader and save — the change appears immediately:
- `assets/shaders/generator.frag` — change the foreground content being fed in
- `assets/shaders/feedback.frag` — change how feedback is processed

The feedback loop resets on the next frame. A very different generator will create a brief "shock" frame before the new trail builds up.

---

## Writing your own generator

Replace `generator.frag` with any GLSL shader. Rules for good feedback input:
- **Black background** — dark areas accumulate feedback, so a very bright background will overwhelm the trails
- **Distinct bright shapes** — SDFs and sharp geometry create clean luma key edges
- **Smooth motion** — fast-jumping content creates choppy trails; slow motion creates silk

The included generator uses signed distance fields (SDF) with smooth-union blending — the same technique as `scheng-raymarcher` in 2D.

---

## Output

Syphon output is always active on macOS as `"scheng-feedback"`. Route to Resolume, VDMX, or any Syphon-compatible application.
