# scheng-edu — Education Toolkit

Five annotated shader lessons. Each lesson teaches one core concept.
Cycle with arrow keys. Edit any shader and save — reloads instantly.

## Run

```bash
cd /Users/tgm/Documents/SPLASH/scheng-edu
cargo run --release
```

## Keys

| Key | Action |
|-----|--------|
| → or ] | Next lesson |
| ← or [ | Previous lesson |
| R | Reload current shader |
| F | Print lesson list |
| Escape | Quit |

## Lessons

| # | File | Teaches |
|---|------|---------|
| 1 | 01_hello_colour.frag | fragColor, uTime, sin(), animation |
| 2 | 02_coordinates.frag | UV space, aspect correction, SDF circle |
| 3 | 03_sdf_shapes.frag | SDF library, union, intersect, subtract, smooth union |
| 4 | 04_domain_warp.frag | Noise, fbm, domain warping, recursion |
| 5 | 05_raymarching.frag | Ray setup, sphere tracing, normals, lighting |

## MIDI

CC1–CC8 wired on every lesson. Each shader documents what each CC does at the top.

## Adding your own shaders

Drop any `.frag` file into `assets/shaders/`. It appears automatically.
Start from `03_sdf_shapes.frag` as a template — it has all the common helpers.
