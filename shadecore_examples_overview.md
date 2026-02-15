# scheng – Examples Overview (23 Real Examples)

This document lists the **23 real, functional examples** in the scheng / schengine repository.
These examples represent the practical surface area of the engine: GPU shaders, graph execution,
video input, OSC control, feedback, and output paths.

---

## A. Core Runtime / Single-Pass (No Graph)

### 1. minimal
**Purpose:** Lowest-level runtime demo. Renders a fullscreen shader directly.
- No graph usage
- Shows basic GL + runtime integration
- Good for shader sketching

### 2. pure_single_pass
**Purpose:** Single fullscreen fragment shader with time + resolution.
- No graph usage
- Focused on procedural visuals

### 3. render_target_only
**Purpose:** Demonstrates offscreen render targets and presentation.
- No graph usage
- Useful for understanding FBO lifecycle

---

## B. Graph Basics (ShaderPass + PixelsOut)

### 4. graph_minimal
**Purpose:** Minimal graph pipeline.
- ShaderPass → PixelsOut
- Baseline for graph execution

### 5. graph_chain2
**Purpose:** Two-pass shader chain.
- Shader A → Shader B → PixelsOut
- Demonstrates multi-pass texture flow

### 6. graph_mixer2
**Purpose:** Two-input mixer.
- Two ShaderPass branches → Crossfade → PixelsOut
- Shows basic video mixing logic

### 7. graph_matrix_mix4
**Purpose:** Four-input matrix mixer.
- 4 inputs → MatrixMix4 → PixelsOut
- Core “video mixer” concept

### 8. graph_mixer_builtin
**Purpose:** Built-in mixer + output abstraction.
- Demonstrates ExecOutputs / sink-style thinking
- Important stepping stone toward sink milestones

---

## C. Texture & Static Sources

### 9. texture_input_minimal
**Purpose:** Bind an external texture as input.
- TextureInputPass → ShaderPass → PixelsOut

### 10. static_source_minimal
**Purpose:** Static image texture input.
- Similar to texture_input_minimal
- Good for testing non-animated sources

---

## D. Feedback / Temporal Effects (Ping-Pong)

### 11. feedback_pingpong
**Purpose:** Classic feedback loop.
- Ping-pong render targets
- Trails, echoes, decay effects

### 12. feedback_orb
**Purpose:** Orb + feedback trails.
- Combines static source + feedback buffer
- OSC-controlled decay/gain

### 13. temporal_slitscan
**Purpose:** Temporal slicing effect.
- Samples different time offsets
- Demonstrates time-based image composition

---

## E. Readback / CPU Interaction

### 14. readback_minimal
**Purpose:** GPU → CPU pixel readback.
- ShaderPass → PixelsOut → ReadbackSink
- Useful for testing, analysis, or export

---

## F. OSC-Controlled Examples

### 15. osc_minimal
**Purpose:** OSC-driven shader parameters.
- Maps OSC messages to uniforms
- Template for live control

---

## G. Syphon / External Output

### 16. syphon_minimal
**Purpose:** Publish GPU output via Syphon.
- ShaderPass → Syphon server
- Connect to VJ tools or OBS

### 17. syphon_patchbay
**Purpose:** Multi-source Syphon patching.
- Acts like a visual routing hub

### 18. syphon_builtin_legacy
**Purpose:** Legacy Syphon output example.
- Historical reference, still functional

---

## H. Video Decode / Capture

### 19. video_decode_source_minimal
**Purpose:** Engine-integrated video decode.
- VideoDecodeSource → ShaderPass → PixelsOut
- Time-controlled via FrameCtx

### 20. video_scrub_keyboard_transport
**Purpose:** Keyboard-driven video transport (Step 12.1).
- Pause / speed / scrub via keyboard
- Showcase example for transport control

### 21. video_source_minimal
**Purpose:** Video input via TextureInputPass.
- Non-engine decode path
- Simpler video binding

### 22. video_device_capture_macos
**Purpose:** macOS camera capture.
- Live device → texture → shader

### 23. webcam_source_minimal
**Purpose:** Webcam passthrough.
- Minimal live capture example

---

## Summary

These 23 examples cover:
- GPU shader execution
- Graph-based rendering
- Multi-pass effects
- Feedback systems
- OSC control
- Video decoding and capture
- External output (Syphon)
- CPU readback

Together, they define the **current practical capability** of scheng.
