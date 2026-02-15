# Examples Index

This folder contains runnable, copy‑pasteable templates for common scheng host/output configurations.

## Quick start

From the repo root:

```bash
# build + run a specific example
cargo run -p scheng-example-syphon-minimal
cargo run -p scheng-example-syphon-patchbay
cargo run -p scheng-example-readback-minimal
```

> Notes
>
> - macOS Syphon examples require `Syphon.framework` to be discoverable at runtime. If you vendor it (recommended), you can run:
>
> ```bash
> export DYLD_FRAMEWORK_PATH="$PWD/vendor"
> ```
>
> - Windows Spout examples will live alongside Syphon (Step 9+).

---

## Step 8 — Output Surface Matrix

Step 8 formalizes how **engine outputs** are routed to different **surfaces/sinks** (preview window, Syphon/Spout, readback, etc.).

- Docs: `docs/OUTPUT_SURFACE_MATRIX.md`
- Diagram: `docs/diagrams/output-surface-matrix.svg`

---

## Example catalog

### Syphon

#### `syphon_minimal`
**Goal:** smallest Syphon sender template (single output → Syphon).  
**Use when:** you want “hello world” Syphon and the fewest moving parts.

Run:

```bash
cargo run -p scheng-example-syphon-minimal
```

#### `syphon_patchbay`
**Goal:** patchbay-style routing (named outputs → multiple sinks), intended as the *template* for “program vs preview” setups.  
**Use when:** you want a stable foundation for switchers, multi-output, and future surface matrix expansion.

Run:

```bash
cargo run -p scheng-example-syphon-patchbay
```

### Readback

#### `readback_minimal`
**Goal:** robust readback template (PixelsOut → ReadbackSink), the foundation for recording/export next.  
**Use when:** you need CPU-visible frames (PNG, video encoder, network streaming, analysis, etc.).

Run:

```bash
cargo run -p scheng-example-readback-minimal
```

---

## Conventions used by these examples

- **Graph stays declarative.** It describes *what* to render; routing is done by sinks in the host.
- **Named outputs are first-class.** Prefer explicit names like `program` and `preview` for anything multi-surface.
- **Sinks are explicit.** Each sink owns “how it consumes frames” (present, Syphon, readback).

If you’re making a new output surface, treat it as:
1) a new sink type, and
2) an entry in the Output Surface Matrix docs.
