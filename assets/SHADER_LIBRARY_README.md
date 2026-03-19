# scheng Shader Library — Phase 6

LZX-inspired analog video synthesis modules implemented as GLSL 330 shaders.

Each shader is a self-contained `.frag` file that works with the scheng compat
header. All use the `iChannel0–3` / `uTime` / `uResolution` convention.

---

## Module Reference

### proc-amp.frag — Processing Amplifier
**Type:** Processor (1 input)

Broadcast-standard video processing in YIQ colour space (NTSC).
Adjusts brightness (luma offset), contrast (luma gain around mid-grey),
saturation (chroma amplitude), and hue (IQ vector rotation).

Hardware reference: Extron DA2 proc amp, Kramer VP-41 series.

```
iChannel0 → [brightness → contrast → saturation → hue rotation] → out
```

| Uniform | Range | Default | Notes |
|---------|-------|---------|-------|
| u_brightness | -1 to 1 | 0.0 | IRE shift |
| u_contrast | 0 to 3 | 1.0 | Gain around 0.5 |
| u_saturation | 0 to 3 | 1.0 | Chroma scale |
| u_hue | -180 to 180 | 0.0 | Degrees |

---

### colorizer.frag — Colour Encoder
**Type:** Processor (1 input)

Maps input luminance to a configurable hue sweep. A luma=0 pixel becomes
`u_hue_start`; luma=1 becomes `u_hue_start + u_hue_range`.
Creates false colour effects, colour encoding of greyscale signals,
and thermal-style visualisations.

Hardware reference: LZX Visual Cortex colouriser, Fairlight CVI colour encoder.

| Uniform | Range | Default | Notes |
|---------|-------|---------|-------|
| u_hue_start | 0–360 | 0.0 | Hue at luma=0 |
| u_hue_range | -360 to 360 | 360.0 | Full sweep |
| u_saturation | 0–1 | 1.0 | |
| u_luminance | 0–1 | 0.5 | Output L in HSL |
| u_invert | 0–1 | 0.0 | Flip luma |

---

### ramp-generator.frag — Ramp Generator
**Type:** Source (no input)

Produces geometric voltage ramps — the fundamental waveform of video synthesis.
Horizontal, vertical, radial, and angular modes. Used as CV inputs to keyers,
oscillators, and mixers to create geometric shapes.

Hardware reference: LZX Cadet I (H ramp), Cadet II (V ramp), Cadet III (radial).

| Uniform | Range | Default | Notes |
|---------|-------|---------|-------|
| u_mode | 0–3 | 0 | 0=H 1=V 2=radial 3=angular |
| u_freq | 0.1–16 | 1.0 | Tile frequency |
| u_phase | 0–1 | 0.0 | Phase offset |
| u_invert | 0–1 | 0.0 | Polarity flip |
| u_center_x/y | 0–1 | 0.5/0.5 | For radial/angular |

---

### luma-keyer.frag — Luma Keyer
**Type:** Mixer (3 inputs)

Extracts a matte from iChannel0 luminance and composites iChannel1 (fg)
over iChannel2 (bg). Soft key uses smoothstep for feathered edges.

Hardware reference: LZX Cadet IV key generator, Panasonic MX-50 keyer.

```
iChannel0 (key source) → [threshold + softness] → matte
matte → mix(iChannel2, iChannel1, matte) → out
```

| Uniform | Range | Default | Notes |
|---------|-------|---------|-------|
| u_thresh | 0–1 | 0.5 | Clip point |
| u_softness | 0–0.5 | 0.05 | Edge feather |
| u_gain | 0–4 | 1.0 | Pre-key amplifier |
| u_invert | 0–1 | 0.0 | Flip matte |

---

### chroma-keyer.frag — Chroma Keyer
**Type:** Mixer (2 inputs)

Keys on a target hue (green screen, blue screen, or custom colour).
Includes spill suppression to remove colour fringing on edges.

| Uniform | Range | Default | Notes |
|---------|-------|---------|-------|
| u_key_hue | 0–360 | 120.0 | 120=green, 240=blue |
| u_hue_range | 0–180 | 30.0 | Acceptance range ± |
| u_saturation | 0–1 | 0.2 | Min sat to key |
| u_softness | 0–1 | 0.1 | Edge feather |
| u_spill_reduce | 0–1 | 0.5 | Spill suppression |

---

### crossfader.frag — T-Bar Crossfader
**Type:** Mixer (2 inputs)

Five transition modes: dissolve, additive, multiply, hard wipe, soft wipe.
`u_tbar` is the primary performance control (map to MIDI CC 7 / mod wheel).

Hardware reference: LZX Cadet VIII, broadcast vision mixer T-bar.

| Uniform | Range | Default | Notes |
|---------|-------|---------|-------|
| u_tbar | 0–1 | 0.5 | 0=full A, 1=full B |
| u_mode | 0–4 | 0 | dissolve/add/mult/hard-wipe/soft-wipe |
| u_softness | 0–0.5 | 0.05 | Wipe edge |

---

### matrix-mixer.frag — Matrix Mixer 4→1
**Type:** Mixer (4 inputs)

Weighted sum of up to 4 channels. Negative gains invert (phase reversal).
DC offset adds a constant bias. The core routing primitive of analog synthesis.

Hardware reference: LZX Matrix Mixer, Buchla 256 spatial sound director.

| Uniform | Range | Default | Notes |
|---------|-------|---------|-------|
| u_gain0–3 | -2 to 2 | 1/0/0/0 | Per-channel gain |
| u_offset | 0–1 | 0.0 | DC bias |
| u_clip | 0–1 | 1.0 | Enable clipping |

---

### feedback.frag — Video Feedback
**Type:** Processor (2 inputs)

Mix live source with a transformed version of the previous frame.
Zoom + rotation + drift create the characteristic feedback spiral.
**Connect iChannel1 to a PreviousFrame node** in the graph.

Hardware reference: LZX Cadet VI lag processor, physical camera-monitor loops.

| Uniform | Range | Default | Notes |
|---------|-------|---------|-------|
| u_decay | 0–0.99 | 0.85 | Feedback tail length |
| u_zoom | 0.9–1.1 | 1.0 | Scale per frame |
| u_rotation | -5 to 5 | 0.0 | Degrees per frame |
| u_offset_x/y | -0.1 to 0.1 | 0 | Drift per frame |
| u_blend_mode | 0–1 | 0 | 0=additive 1=mix |

---

### pattern-generator.frag — Pattern Generator
**Type:** Source (no input)

SMPTE colour bars (75% and 100%), grid, crosshatch, test card, circle, checkerboard.
Essential for calibration and as CV source signals.

Hardware reference: Tektronix 1910 signal generator, EIA RS-189 colour bars.

| u_mode | Pattern |
|--------|---------|
| 0 | SMPTE 75% colour bars |
| 1 | 100% colour bars |
| 2 | Grid |
| 3 | Crosshatch (±45°) |
| 4 | Luma + hue ramp test card |
| 5 | Filled circle |
| 6 | Checkerboard |

---

### waveform-monitor.frag — Waveform Monitor
**Type:** Processor (1 input)

Plots signal levels as a broadcast waveform trace. Three modes: luma parade,
RGB parade (R/G/B side by side), and overlay on the source image.

Hardware reference: Tektronix 1740, Leader LV-5800.

| Uniform | Range | Default | Notes |
|---------|-------|---------|-------|
| u_mode | 0–2 | 0 | 0=luma 1=RGB 2=overlay |
| u_intensity | 0–1 | 0.8 | Trace brightness |
| u_persistence | 0–32 | 1.5 | Trace thickness |

---

### vectorscope.frag — Vectorscope
**Type:** Processor (1 input)

Plots each pixel as a point on the Cb/Cr (chroma) plane.
Angle = hue, radius = saturation. Shows SMPTE colour bar targets,
axis graticule, and the broadcast skin-tone line at ~123°.

Hardware reference: Tektronix 1760, Leader VC-5800.

| Uniform | Range | Default | Notes |
|---------|-------|---------|-------|
| u_gain | 0.1–4 | 1.0 | Plot brightness |
| u_graticule | 0–1 | 1.0 | Show targets + axes |
| u_skin_line | 0–1 | 1.0 | Skin-tone indicator |

---

## Signal Chain Examples

### Classic proc amp → colorizer
```
PatternGenerator → ProcAmp → Colorizer → PixelsOut
```
Generate greyscale bars, adjust luma, recolour by luminance mapping.

### Luma-keyed composite
```
ShaderSource (fg) ──────────────────────────────▶ LumaKeyer (iCh1)
RampGenerator (key) ─────────────────────────────▶ LumaKeyer (iCh0)
ShaderSource (bg) ────────────────────────────────▶ LumaKeyer (iCh2)
LumaKeyer ──────────────────────────────────────▶ ProcAmp → PixelsOut
```

### Video feedback spiral
```
ShaderSource ──────────────────────────────────▶ Feedback (iCh0)
PreviousFrame ─────────────────────────────────▶ Feedback (iCh1)
Feedback → Colorizer → PixelsOut
```

### Full chain with monitoring
```
PatternGenerator ──▶ ProcAmp ──▶ Crossfader (A)
ShaderSource ──────────────────▶ Crossfader (B)
Crossfader ──▶ MatrixMixer ──▶ PixelsOut (main)
MatrixMixer ──▶ WaveformMonitor ──▶ PixelsOut (monitor)
MatrixMixer ──▶ Vectorscope ──▶ PixelsOut (vectorscope)
```

---

## Adding to your instrument

Copy the relevant shader to `assets/shaders/` and add the params block
from `params-library.json` to your `assets/params.json`.

```bash
cp proc-amp.frag         assets/shaders/
cp crossfader.frag       assets/shaders/
cp pattern-generator.frag assets/shaders/
```

The scheng hot-reload watcher picks up changes instantly — no restart needed.
