# scheng-playground

<p align="center">
  <img src="https://img.shields.io/badge/scheng-template-3b82f6?style=flat-square"/>
  <img src="https://img.shields.io/badge/complexity-explorer-8b5cf6?style=flat-square"/>
  <img src="https://img.shields.io/badge/hot--reload-yes-06b6d4?style=flat-square"/>
  <img src="https://img.shields.io/badge/MIDI-CC1--CC8-f59e0b?style=flat-square"/>
</p>

Interactive multi-shader explorer. Drop any `.frag` file into `assets/shaders/`, cycle through them with arrow keys, and all 8 MIDI CC params are pre-wired to every shader.

---

## Run

```bash
cargo run --release
cargo run --release -- --width 1920 --height 1080
```

---

## Keyboard

| Key | Action |
|-----|--------|
| `→` or `]` | Next shader |
| `←` or `[` | Previous shader |
| `R` | Reload all shaders from disk |
| `F` | Print shader list to terminal |
| `Escape` | Quit |

The window title always shows the current shader name and index.

---

## MIDI

CC1–CC8 map to `u_p1`–`u_p8` in every shader (range 0.0–1.0, smoothed).

Declare only the params you actually use at the top of your shader:

```glsl
uniform float u_p1;  // CC1 — your description here
uniform float u_p2;  // CC2
```

All 8 params are always uploaded to every shader — you just choose which ones to declare and use.

---

## Writing your own shader

Start from `assets/shaders/08_template.frag`. All standard scheng uniforms are available without declaration:

```glsl
// Available in every shader — no declaration needed:
// uTime        float   seconds since start
// uFrame       uint    frame counter
// uResolution  vec2    width × height in pixels
// v_uv         vec2    UV coordinates [0,1]
// fragColor    vec4    write your pixel here

uniform float u_p1;  // declare the params you want

void main() {
    vec2  uv  = v_uv;
    float t   = uTime;

    // your shader here

    fragColor = vec4(uv, u_p1, 1.0);
}
```

Save the file — it hot-reloads instantly. No restart needed.

---

## Included shaders

| Shader | Description | Key params |
|--------|-------------|-----------|
| `01_gradient` | Animated color gradient — the hello world | CC1=speed, CC2=hue, CC3=frequency |
| `02_plasma` | Layered sine-wave interference | CC1=speed, CC2=scale, CC3=hue |
| `03_tunnel` | Rotating geometric tunnel | CC1=fly speed, CC2=rotation, CC3=sectors |
| `04_noise` | Fractal Brownian motion (fBm) | CC1=speed, CC2=scale, CC3=octaves, CC4=warp |
| `05_sdf2d` | 2D signed distance fields | CC1=speed, CC2=orbit, CC3=blend radius |
| `06_voronoi` | Voronoi cellular noise | CC1=speed, CC2=scale, CC3=edge sharpness |
| `07_lines` | Interference / moiré lines | CC1=speed, CC2=frequency, CC3=rotation |
| `08_template` | Blank starting point — copy this | all yours |

Files are loaded in alphabetical order. Prefix with numbers to control order.

---

## Adding your own shaders

Just drop `.frag` files into `assets/shaders/`. Press `R` to reload without restarting. The playground picks up new files automatically.

Naming tip: prefix with a number so they sort cleanly:
```
assets/shaders/
  01_gradient.frag
  02_plasma.frag
  09_my_experiment.frag   ← your new shader appears at position 9
  10_another_idea.frag
```

---

## Output

Syphon output is always active on macOS as `"scheng-playground"`. The currently displayed shader is always what Syphon sends — switching shaders switches the Syphon output instantly.

---

## Full documentation

[SDK Reference](https://yourusername.github.io/scheng/sdk-reference.html)
