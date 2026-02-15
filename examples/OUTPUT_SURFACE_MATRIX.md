# Output Surface Matrix (Step 8)

This document defines the **Output Surface Matrix** used by scheng v1.

It answers a critical question:

> Where can pixels go once a graph is executed?

scheng deliberately separates **graph semantics** from **output surfaces**.
Graphs produce pixels. Output sinks decide what happens next.

---

## Core Rule (v1 Invariant)

`PixelsOut` nodes **MUST** receive input from a render pass
(`ShaderPass` or `Mixer`).

Direct texture → output routing is intentionally disallowed.

This guarantees:

- Deterministic execution
- Explicit GPU ownership
- Portable output behavior

---

## Output Surface Matrix

| Surface Type | Sink | Primary Use | Real-Time | Persistent | External |
|-------------|------|-------------|-----------|------------|----------|
| Window | HostWindow | Preview / Debug | Yes | No | No |
| Syphon | `SyphonSink` | macOS GPU sharing | Yes | No | Yes |
| Patchbay | `PatchbaySink` | Multi-route fan-out | Yes | No | Yes |
| Readback | `ReadbackSink` | Recording / Export | No | Yes | No |

---

## Mental Model

```
[ ShaderPass / Mixer ]
            ↓
        PixelsOut
            ↓
        Output Sink
```

The same graph can feed multiple sinks via Patchbay.

---

## Sink Semantics

### Window (Preview)

- Driven by `scheng-host-winit`
- Exists only to host a GL context
- Not a recording or sharing surface
- Optional in headless workflows

### Syphon

- macOS-only GPU texture sharing
- Zero-copy
- Ideal for OBS, Resolume, TouchDesigner

Constraints:

- GPU-only
- No frame persistence
- Requires render pass upstream

### Patchbay

- Explicit routing layer
- One input → many outputs
- Can feed:
  - Syphon
  - Preview window
  - Readback
  - Future sinks (Spout, NDI)

This is the fan-out backbone.

### Readback

- GPU → CPU transfer
- Explicit, typically synchronous
- Foundation for:
  - Video encoding
  - Image export
  - Offline processing

Tradeoffs:

- Slower
- Blocking
- Deterministic

---

## Why This Matters

The matrix prevents accidental coupling:

- Rendering logic mixed with output logic
- Hidden side effects
- Platform-specific hacks

Instead:

- One graph
- Many surfaces
- Clear ownership
- Future-proof extension

---

## Extension Points (Future)

- Spout (Windows)
- NDI
- PipeWire
- Vulkan interop
- Headless batch export

All slot into the same matrix.

---

## Summary

Think in surfaces, not destinations.

The graph doesn’t know where pixels go —
only how they are produced.

That separation is the core design win of scheng v1.
