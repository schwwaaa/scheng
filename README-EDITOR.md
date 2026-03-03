# scheng-editor

A browser-based node graph editor for **scheng** — a real-time GLSL video synthesis and compositing engine. Wire nodes together, load shaders from the library, and send the compiled graph to the Rust runtime over WebSocket. Designed for live performance, broadcast-style signal routing, and hardware-inspired video synthesis.

---

## Architecture

```
scheng-editor.html          ← this file (zero dependencies, single HTML)
        │
        │  WebSocket ws://127.0.0.1:7777
        ▼
scheng-graph (Rust bridge)  ← validates graph, manages node state
        │
        ▼
scheng-runtime-glow         ← OpenGL render loop, GLSL compiler, texture binding
```

The editor is entirely self-contained. Open `scheng-editor.html` in a browser — no server, no build step, no npm. Connect to the bridge and start patching.

---

## Quick Start

1. Start the scheng bridge: `cargo run --release`
2. Open `scheng-editor.html` in Chrome or Firefox
3. Click **connect** — the dot turns green
4. Pick a template from the **templates** tab and click it
5. Click **▶ compile** — video starts rendering

---

## Interface

### Canvas

| Action | Input |
|---|---|
| Pan | Middle-mouse drag, or Space + drag |
| Zoom | Scroll wheel |
| Select node | Left click |
| Move node | Left drag on node header |
| Delete node | Select → `Del` or `Backspace` |
| Draw wire | Drag from an **output** port — cursor changes to crosshair |
| Complete wire | Click an **input** port |
| Cancel wire | `Esc` |
| Delete wire | Click on wire to select (turns yellow) → `Del` |

### Panel (right sidebar)

| Tab | Purpose |
|---|---|
| **nodes** | Spawn I/O and processor nodes from the bridge registry |
| **fx library** | Click any item to add a pre-loaded shader node |
| **templates** | One-click full patch presets |
| **properties** | Selected node — uniforms, GLSL editor, port map |
| **validate** | Live checks: WebSocket, node count, PixelsOut, compile status |
| **log** | Bridge messages, errors, compile confirmations |

Click the **properties** header to collapse it when you need more canvas space — the selected node label stays visible in the header bar.

---

## Node Types

These are the node kinds the bridge accepts. Everything else is a shader loaded onto one of these.

| Kind | Ports | Description |
|---|---|---|
| `shader_pass` | `in` → `out` | Single-input GLSL processor |
| `shader_source` | → `out` | Generator — no input required |
| `crossfade` | `a`, `b` → `out` | 2-input mixer. Custom GLSL loaded via FX library |
| `matrix_mix4` | `in0`–`in3` → `out` | 4-input mixer. Custom GLSL loaded via FX library |
| `add` | `a`, `b` → `out` | Additive blend — native, no custom shader |
| `multiply` | `a`, `b` → `out` | Multiply blend — native, no custom shader |
| `pixels_out` | `in` | Final output to render target — every graph needs one |
| `noise_source` | → `out` | Built-in noise generator |
| `color_correct` | `in` → `out` | Built-in colour correction |
| `blur` | `in` → `out` | Built-in Gaussian blur |
| `keyer` | `in` → `out` | Built-in luma keyer |
| `feedback` | `in` → `out` | Frame feedback loop |
| `syphon` | `in` | Output to Syphon server (macOS) |
| `window` | `in` | Output to a display window |

> **Crossfade and matrix_mix4 are in the FX library, not the node palette.** Spawn them from the purple "spawn a mixer node" buttons at the top of the mixers section, or use any template that includes them.

### Port → iChannel mapping

When a custom GLSL shader is loaded onto a mixer node, ports map to texture samplers as follows:

| Node kind | Port | GLSL sampler |
|---|---|---|
| `crossfade` | `a` | `iChannel0` |
| `crossfade` | `b` | `iChannel1` |
| `matrix_mix4` | `in0` | `iChannel0` |
| `matrix_mix4` | `in1` | `iChannel1` |
| `matrix_mix4` | `in2` | `iChannel2` |
| `matrix_mix4` | `in3` | `iChannel3` |
| `shader_pass` | `in` | `iChannel0` |

---

## Writing Shaders

All fragment shaders must match the runtime's vertex output exactly:

```glsl
#version 330 core
in vec2 v_uv;           // ← must be v_uv (lowercase), NOT vUV
out vec4 fragColor;     // output name can be anything

uniform sampler2D iChannel0;  // input texture(s)
uniform float uTime;          // seconds since start
uniform vec2 uResolution;     // output dimensions in pixels
uniform float u_myParam;      // your uniforms — u_ prefix

void main() {
    vec2 uv = v_uv;
    fragColor = vec4(uv, 0.5 + 0.5 * sin(uTime), 1.0);
}
```

**Uniform naming conventions**

- `u_snake_case` — all custom uniforms use this prefix
- `uTime` — time in seconds (also aliased as `u_time`)
- `uResolution` — output size (also aliased as `u_resolution`)
- `iChannel0` … `iChannel3` — input textures (sampler names are fixed)

**Uniform ranges** — the editor auto-detects range from the name. You can document the default in a comment: `uniform float u_gain; // gain (1.0)`. Common auto-range heuristics:

| Name contains | Range |
|---|---|
| `thresh`, `clip`, `alpha` | 0 – 1 |
| `gain`, `level`, `bright` | 0 – 2 |
| `hue`, `angle` | 0 – 1 (hue), 0 – 360 (angle) |
| `freq`, `scale` | 0.1 – 20 |
| `speed` | 0 – 3 |
| `soft`, `blur`, `radius` | 0 – 0.2 |

---

## GLSL Editor (Properties Panel)

Select any shader node to open the GLSL editor. Make edits, then click **▶ apply + compile** to send to the bridge and recompile the graph.

> **Autosave** writes to `localStorage` on every keystroke (1.5 s debounce) — it does **not** recompile and does **not** move nodes. Compile only happens when you click the button.

The **mix** slider (on native crossfade nodes) controls the bridge's built-in blend parameter. It is hidden when a custom GLSL shader is loaded — use the `u_tbar` uniform slider instead.

---

## Templates

Templates spawn a complete pre-wired patch with named nodes, edges, shaders, and default uniform values, then compile automatically.

### Foundational modules

| Template | Description |
|---|---|
| **T-Bar Crossfader** | `crossfade` node with T-bar shader. `u_tbar` 0→1 = A→B |
| **Patch Mix 4-input** | `matrix_mix4` with independent `u_a/b/c/d` gains. Start `u_a=1`, bring up others |
| **Chroma Key** | HSV hue-distance keyer. `u_hue` 0.33=green 0.5=blue. `u_thresh`/`u_soft`/`u_spill` |
| **Key-Over 3-input** | Luma key with separate BG, fill, and key signal. Self-key: wire fill to both in1 and in2 |
| **Wipe Transition** | Auto-animates with `uTime`. Set `u_speed=0` for manual `u_pos` control. `u_angle` sets direction |

### Program buses

| Template | Description |
|---|---|
| **T-bar crossfader** | Program bus A/B with T-bar |
| **Wipe bus** | Pattern-reveal wipe between two sources |
| **Key-over bus** | 3-input luma key |
| **Chroma key bus** | Greenscreen/bluescreen |
| **DSK + program** | Downstream keyer — graphic over program |
| **Multi-view monitor** | 4-source confidence display |

### LZX-style modules

| Template | Description |
|---|---|
| **LZX Waveform Patch** | H + V + XY waveform generators mixed through Patch Mix |
| **Video Blend Matrix** | Per-channel R/G/B blend mode demo — try different modes on each channel |
| **Triple Keyer** | 3 independent luma key layers stacked over background |
| **ShapeChanger** | Geometry warp + colour remap — radial / spiral / kaleidoscope / tunnel / polar |
| **3D Spin** | Full-rotation dual-face spin. `u_speed=0` + scrub `u_angle` for manual control |

### Framebuffer architectures

| Template | Description |
|---|---|
| **Fairlight CVI Style** | Dual bus → T-bar → proc amp → DSK → output |
| **Video Toaster Style** | 4-input switcher → DVE M/E → DSK |
| **Internal Feedback Loop** | Source mixed with processed previous frame — `u_tbar` controls intensity |
| **Mixer Self-Feedback** | A/B bus with separate feedback arm |
| **Video Echo Machine** | Three independent delay taps (short/medium/long) summed by Patch Mix |
| **Dual Bus A/B** | Separate preview and program buses, DSK over program |

### CVI, video synth, broadcast, scope, keying, DVE, Rutt/Etra, LFO — 60+ additional templates

Open the **templates** tab and browse by section header.

---

## FX Library

### ★ Modules — standalone processors

Pre-configured nodes that work immediately off the shelf.

| Shader | Kind | Key uniforms |
|---|---|---|
| `tbar_module` | crossfade | `u_tbar` `u_curve` `u_gain` |
| `patch_mix_module` | matrix_mix4 | `u_a` `u_b` `u_c` `u_d` `u_mode` |
| `luma_key_module` | crossfade | `u_low` `u_high` `u_soft` `u_invert` |
| `chroma_key_module` | crossfade | `u_hue` `u_thresh` `u_soft` `u_spill` |
| `wipe_module` | crossfade | `u_pos` `u_mode` `u_angle` `u_soft` |
| `dsk_module` | crossfade | `u_density` `u_clip` `u_gain` |

### Generators

No input required — use `shader_source` or `shader_pass` nodes.

| Shader | Description |
|---|---|
| `gen_plasma` | Animated plasma field |
| `gen_sdf_orbs` | SDF sphere field with glow |
| `gen_grid` | Vector grid `u_cols` `u_rows` |
| `gen_noise` | Animated value noise |
| `gen_lissajous` | Lissajous scope figure |
| `gen_interference` | Moiré / interference pattern |
| `gen_scope` | Oscilloscope beam |
| `gen_cellular` | Cellular automata |
| `gen_vectorscope` | IQ vectorscope display |
| `gen_testcard` | Broadcast test card / colour bars |
| `gen_ramp_h` | Horizontal sync ramp (LZX) |
| `gen_ramp_v` | Vertical sync ramp (LZX) |
| `gen_osc_shape` | Geometric shape oscillator — circle/rect/diamond/ring (LZX Sensory) |
| `gen_quad_osc` | Quadrature RGB sine oscillator (LZX) |
| `gen_patch_matrix` | 3 independent RGB oscillators — R/G/B patchable (LZX) |
| `gen_voltage_texture` | Voltage-controlled noise field (LZX Cadet) |
| `lzx_waveform_h` | Horizontal waveform — saw/tri/sine/square/pulse with DC offset |
| `lzx_waveform_v` | Vertical waveform — same, Y axis |
| `lzx_waveform_xy` | XY waveform matrix — independent X+Y into RGB composite |
| `gen_triangle_wave` | Triangle wave oscillator |
| `gen_sawtooth` | Sawtooth ramp |
| `gen_kaleid_osc` | Kaleidoscope oscillator |
| `gen_ring_mod` | Ring modulator pattern |

### Colour / grade

| Shader | Description |
|---|---|
| `fx_colour_correct` | Lift / gamma / gain / sat / hue (broadcast LGG) |
| `fx_colorizer` | CVI-style luma-to-colour mapping |
| `fx_jones` | Jones colorizer — luma banding |
| `fx_hue_rot` | Hue rotation (YIQ) |
| `fx_posterize` | Posterization |
| `fx_solarize` | Solarize |
| `fx_invert` | Colour invert |
| `fx_mirror` | Mirror / kaleidoscope |
| `fx_rgb_split` | RGB channel split / chromatic aberration |
| `fx_pixel_sort` | Pixel sort / glitch |
| `fx_vhs` | VHS tape degradation |
| `fx_bloom` | Bloom / glow |

### Keying (crossfade — ports a=BG, b=FG)

| Shader | Description |
|---|---|
| `fx_luma_key` | Luma key — `u_low` `u_high` `u_soft` |
| `fx_chroma_key` | HSV chroma key — `u_hue` `u_thresh` `u_soft` `u_spill` |
| `fx_downstream_key` | DSK — `u_density` `u_clip` |
| `fx_linear_key` | Linear key (fill + separate key signal) |
| `fx_self_key` | Self-key — subject keys over black |
| `key_edge_matte` | Edge matte key |
| `key_difference` | Difference key |
| `key_colour_replace` | Colour replace / chroma swap |
| `fx_matte_key` | Matte key (external alpha) |

### Mix / blend (crossfade — ports a + b)

| Shader | Blend mode |
|---|---|
| `comp_add` | Additive |
| `comp_multiply` | Multiply |
| `comp_screen` | Screen |
| `comp_overlay` | Overlay |
| `comp_difference` | Difference / XOR |
| `comp_hardlight` | Hard light |
| `comp_softlight` | Soft light |
| `comp_dissolve` | Dissolve / noise mix |

### Compositors & colorspace

| Shader | Kind | Description |
|---|---|---|
| `video_blend_matrix` | crossfade | 16 blend modes selectable per R/G/B channel independently. `u_mode_r/g/b`: 0=add 1=multiply 2=screen 3=difference 4=exclusion 5=overlay 6=hardlight 7=softlight 8=darken 9=lighten 10=subtract 11=divide 12=min 13=max |
| `triple_keyer` | matrix_mix4 | 3 independent luma key layers over BG. `u_thresh1/2/3` `u_gain1/2/3` `u_soft` `u_order` |
| `shapechanger` | shader_pass | UV remapping + colour remap. `u_shape`: 0=radial 1=spiral 2=kaleid 3=tunnel 4=polar. `u_remap`: 0=pass 1=HSV rotate 2=posterize |
| `comp_rgb` | matrix_mix4 | 3 streams → single RGB composite |
| `comp_multikey` | matrix_mix4 | BG + 3 keyed layers |
| `util_colorspace` | shader_pass | Hue/sat/val remap |

### DVE — digital video effects (crossfade — ports a=from, b=to)

| Shader | Description |
|---|---|
| `fx_3d_spin` | Full Y/X/Z axis rotation with perspective shading. `u_speed=0` + scrub `u_angle` for manual |
| `fx_dve_cube` | Cube spin — Video Toaster / Ampex ADO style |
| `fx_dve_spin` | Spin / tumble — Quantel Harry style |
| `fx_dve_flip` | Page flip H or V |
| `fx_dve_push` | Push reveal |
| `fx_dve_squeeze` | Squeeze transition |
| `fx_dve_split` | Barn-door split |
| `fx_dve_pip` | Picture-in-picture — `u_x` `u_y` `u_w` `u_h` |
| `fx_dve_zoom_reveal` | Zoom reveal |
| `fx_dve_burst` | Burst / star wipe |

### Wipes (crossfade — ports a=source, b=target)

| Shader | Description |
|---|---|
| `fx_wipe_linear` | Linear wipe — `u_speed>0` auto-animates, `u_speed=0` manual via `u_pos`. `u_angle` sets direction |
| `fx_wipe_radial` | Radial / clock wipe |
| `fx_wipe_iris` | Iris / diamond wipe |
| `fx_wipe_slide` | Push / slide wipe |

### Warp / geometry

| Shader | Description |
|---|---|
| `fx_dve_transform` | Scale + rotate + translate |
| `fx_feedback_zoom` | Feedback zoom trail |
| `fx_rutt_etra` | Rutt/Etra scanline displacement |
| `fx_scanimate` | Scanimate envelope warp |
| `fx_zoom_blur` | Radial zoom blur |
| `fx_barrel` | Barrel / pincushion |
| `fx_polar` | Rectangular ↔ polar |
| `fx_tiles` | Tile repeat |

### Video synth — analog systems

| Shader | Description |
|---|---|
| `fx_videosynth` | Waveform displacement synthesizer |
| `fx_paik_abe` | Paik/Abe synth — colour feedback |
| `fx_wobulator` | Wobulator / sync roll |
| `fx_vector_raster` | Vector-to-raster scan conversion |
| `fx_analogue_noise` | Analogue noise / snow |

### Post / film / CRT

| Shader | Description |
|---|---|
| `fx_edge` | Edge detect (Sobel) |
| `fx_glow` | Glow / bloom |
| `fx_scanlines` | Scanline overlay |
| `fx_crt_curve` | CRT barrel + vignette + mask |
| `fx_vhs_glitch` | VHS tape glitch |
| `fx_chroma_aberration` | Chromatic aberration |
| `fx_film_grain` | Film grain |
| `fx_dither` | Bayer ordered dither |
| `fx_halftone` | Halftone dots |
| `fx_paint_smear` | Paint smear / brush |
| `fx_mosaic` | Mosaic / pixelate |
| `fx_trails` | Trails / decay |

### Feedback / framebuffer

| Shader | Description |
|---|---|
| `fx_feedback_trails` | Decay + zoom + rotate + saturation — route output back to input |
| `fx_echo_tap` | Single delay tap with blur — use multiple in parallel for multi-tap echo |

### Output / monitor utilities

| Shader | Description |
|---|---|
| `util_passthrough` | Passthrough with gain |
| `util_scope` | Waveform scope overlay |
| `util_histogram` | Histogram parade |
| `util_false_colour` | False colour exposure analysis |
| `util_zebra` | Zebra stripes |
| `fx_aa_output` | FXAA anti-aliasing on output |

---

## Saving and Loading Patches

### Save

Click **↓ save patch** in the header. The current graph downloads as a `.scheng.json` file — node positions, edges, shader GLSL, and all uniform values are included. Nothing is stored in the browser.

### Load / Import

Click **↑ import** (the file input button in the header). Any `.scheng.json` file you previously exported loads immediately and recompiles.

### Format

```json
{
  "name": "my patch",
  "saved": "2026-03-02T04:00:00.000Z",
  "nodes": [
    {
      "id": "src_1",
      "kind": "shader_source",
      "label": "Plasma",
      "position": { "x": 60, "y": 175 },
      "frag": "#version 330 core\n...",
      "uniforms": { "u_scale": 3.0, "u_speed": 0.8 }
    }
  ],
  "edges": [
    { "from_id": "src_1", "from_port": "out", "to_id": "out_1", "to_port": "in" }
  ]
}
```

---

## OSC / MIDI Control

Every uniform slider is live-addressable over OSC:

```
/scheng/node/<node_id>/uniform/<uniform_name>  <float_value>
```

**Examples**
```
/scheng/node/xfad/uniform/u_tbar  0.75
/scheng/node/key/uniform/u_thresh  0.35
/scheng/node/proc/uniform/u_gain  1.2
```

Short form (node label only):
```
/<node_label>/<uniform_name>  <value>
```

The OSC address is shown as a tooltip on every slider — hover to see it.

---

## WebSocket Protocol

The bridge listens on `ws://127.0.0.1:7777`. All messages are JSON.

### Editor → Bridge

```json
{ "action": "add_node", "id": "gen_1", "kind": "shader_source",
  "label": "Plasma", "position": { "x": 100, "y": 100 } }

{ "action": "connect", "from_id": "gen_1", "from_port": "out",
  "to_id": "out_1", "to_port": "in" }

{ "action": "set_shader", "node_id": "gen_1",
  "frag": "#version 330 core\n..." }

{ "action": "set_uniform", "node_id": "gen_1",
  "name": "u_scale", "value": 4.0 }

{ "action": "set_mix", "node_id": "xfad", "mix": 0.5 }

{ "action": "set_weights", "node_id": "mx",
  "weights": [0.5, 0.3, 0.2, 0.0] }

{ "action": "compile" }

{ "action": "remove_node", "id": "gen_1" }

{ "action": "disconnect", "from_id": "gen_1", "from_port": "out",
  "to_id": "out_1", "to_port": "in" }
```

### Bridge → Editor

```json
{ "type": "registry", "nodes": [ ... ] }
{ "type": "node_added", "id": "gen_1", "kind": "shader_source", ... }
{ "type": "node_removed", "id": "gen_1" }
{ "type": "edge_added", "from_id": "...", "from_port": "...", ... }
{ "type": "compiled", "ok": true }
{ "type": "error", "message": "Link: vUV not written by vertex shader" }
{ "type": "param_updated", "node_id": "...", "name": "u_scale", "value": 4.0 }
```

---

## Troubleshooting

**`unknown variant 'shader_mix_2'`** — your saved patch contains an old node kind from a previous version. Load the patch, open each node, and change its kind to `crossfade` (2-input) or `matrix_mix4` (3–4 input). Or re-export it — all current templates use only valid kinds.

**`vUV not read by vertex shader`** — a shader has `in vec2 vUV` but the runtime vertex shader outputs `v_uv` (lowercase). The editor auto-corrects this when loading `.scheng.json` patches, but if you paste shader code manually use `in vec2 v_uv`.

**`node 'xyz' not found`** — a template tried to connect a node before `add_node` was acknowledged. This usually resolves on recompile; if not, close the template and try again.

**Blank output from Patch Mix** — check that `u_a` is set to `1.0`. The default is `1.0` for `u_a` and `0.0` for `u_b/c/d`. Bring gains up gradually — there is no normalisation, so setting all four to `1.0` may clip.

**Wipe is frozen** — `u_speed` is `0.0`. Either raise `u_speed` for auto-animation, or drag `u_pos` manually from `0` to `1`.

**Chroma key not firing** — `u_hue` targets green by default (`0.33`). For blue screen set `u_hue=0.5`, red screen `u_hue=0.0`. Raise `u_thresh` to expand the key area. Lower `u_soft` for a harder edge.

**Key-Over in1/in2 no signal** — confirm source nodes are connected to `in1` and `in2` (not `b` and `c` — `matrix_mix4` uses `in0/in1/in2/in3`). Lower `u_thresh` and raise `u_gain` — the key signal source must have enough luminance to cross the threshold.

**UI lagging / slow** — avoid saving patches to browser storage (the editor no longer does this by default). If you have many nodes, collapse the Properties panel when not editing.

---

## Totals

| Item | Count |
|---|---|
| GLSL shaders | 128 |
| Templates | 82 |
| Valid bridge node kinds | 14 |
| FX library categories | 12 |

---

## File

`scheng-editor.html` — single file, zero dependencies, ~6,600 lines. Open directly in browser.
