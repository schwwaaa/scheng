# scheng-raymarcher

<p align="center">
  <img src="https://img.shields.io/badge/scheng-template-3b82f6?style=flat-square"/>
  <img src="https://img.shields.io/badge/rendering-raymarching-8b5cf6?style=flat-square"/>
  <img src="https://img.shields.io/badge/3D-SDF%20geometry-06b6d4?style=flat-square"/>
  <img src="https://img.shields.io/badge/MIDI-CC1--CC8-f59e0b?style=flat-square"/>
</p>

A fully 3D raymarched scene running inside a single fragment shader. No vertex buffers. No mesh data. No depth buffer. Every pixel independently traces a ray into a Signed Distance Field (SDF) scene and finds its own surface.

---

## How 3D works here

Traditional 3D rendering submits triangle geometry to the GPU and rasterizes it. scheng uses **raymarching** instead — the GPU executes a fragment shader for every pixel, and each pixel casts a ray into an implicit surface description.

```
for each pixel:
    cast ray from camera through pixel
    step along ray in small increments
    at each step: evaluate SDF to find nearest surface distance
    if distance < threshold: HIT → shade the surface
    if step count exceeded: MISS → shade the background
```

The SDF evaluates all geometry analytically — spheres, boxes, tori, and their smooth blends — with no triangle approximation. This gives perfectly smooth surfaces and free operations like smooth-union that would be impossible with meshes.

The entire scene — geometry, camera, lighting, shadows, AO — lives in `assets/shaders/scene.frag`. Hot-reload as fast as any 2D shader.

---

## Run

```bash
cargo run --release
cargo run --release -- --width 1920 --height 1080
cargo run --release -- --width 3840 --height 2160 --msaa 4
```

---

## MIDI controls

| CC | Parameter | Effect |
|----|-----------|--------|
| CC1 | Camera orbit | Rotates camera around scene (0–360°) |
| CC2 | Camera elevation | Moves camera up/down |
| CC3 | Camera distance | Zooms in/out |
| CC4 | Fog density | Atmospheric depth haze |
| CC5 | Scene complexity | Morphs and animates geometry |
| CC6 | Light temperature | Warm tungsten → cool daylight |
| CC7 | Reflectivity | Matte surface → mirror |
| CC8 | Animation speed | Slows or accelerates all motion |

All parameters are smoothed (smooth=0.05–0.08) so fader moves feel continuous.

---

## Scene contents

The default scene contains:
- **Central morphing sphere** — twists and pulses with time, iridescent surface
- **Two orbiting tori** — rotate in different planes, electric blue/violet
- **Floating octahedra** — scattered through space, warm gold
- **Reflective floor** — checker pattern, receives shadows from all objects

All objects are composed using **smooth-union** (`smin`) — surfaces blend into each other with a soft radius rather than hard intersections. This is a capability specific to SDF geometry.

---

## Shader techniques

| Technique | Purpose |
|-----------|---------|
| Raymarching | Core 3D rendering loop — 128 max steps |
| SDF primitives | Sphere, torus, box, cylinder, octahedron |
| Smooth-union (`smin`) | Organic blending of surfaces |
| Smooth-subtraction (`smax`) | Carving holes with soft edges |
| Normal estimation | Central differences — 6 SDF samples |
| Soft shadows | Secondary ray march toward light |
| Ambient occlusion | 5-sample AO along surface normal |
| Blinn-Phong + SSS | Specular highlight + subsurface glow |
| ACES tone mapping | Filmic contrast and highlight roll-off |
| Gamma correction | sRGB output (γ = 2.2) |
| Vignette | Subtle edge darkening |

---

## Project layout

```
scheng-raymarcher/
├── Cargo.toml
├── build.rs
├── src/
│   └── main.rs
└── assets/
    └── shaders/
        └── scene.frag    ← edit this live
```

Place next to the `scheng/` workspace:

```
projects/
  scheng/
  scheng-raymarcher/
```

---

## Writing your own SDF scene

Drop in any SDF from [Inigo Quilez's SDF library](https://iquilezles.org/articles/distfunctions/) and add it to the `scene()` function:

```glsl
// Add a new primitive
float dMyShape = sdCapsule(p - vec3(2.0, 0.0, 0.0), 0.5, 0.8);

// Blend it into the scene with smooth-union (k=0.3 = blend radius)
dAll = smin(dAll, dMyShape, 0.3);
```

Hot-reload shows the result immediately on save.

---

## Output

Syphon output is always active on macOS as `"scheng-raymarcher"`. Receive it in Resolume, VDMX, or any Syphon-compatible application.

---

## Full documentation

[SDK Reference](https://yourusername.github.io/scheng/sdk-reference.html)
