# scheng – Example Run Commands

This document lists **how to run each of the 23 real scheng examples** from the repository.
All commands assume you are in the repo root.

---

## A. Core Runtime / Single-Pass (No Graph)

### 1. minimal
```bash
cargo run -p scheng-example-minimal
```

### 2. pure_single_pass
```bash
cargo run -p scheng-example-pure-single-pass
```

### 3. render_target_only
```bash
cargo run -p scheng-example-render-target-only
```

---

## B. Graph Basics

### 4. graph_minimal
```bash
cargo run -p scheng-example-graph-minimal
```

### 5. graph_chain2
```bash
cargo run -p scheng-example-graph-chain2
```

### 6. graph_mixer2
```bash
cargo run -p scheng-example-graph-mixer2
```

### 7. graph_matrix_mix4
```bash
cargo run -p scheng-example-graph-matrix-mix4
```

### 8. graph_mixer_builtin
```bash
cargo run -p scheng-example-graph-mixer-builtin
```

---

## C. Texture & Static Sources

### 9. texture_input_minimal
```bash
cargo run -p scheng-example-texture-input-minimal
```

### 10. static_source_minimal
```bash
cargo run -p scheng-example-static-source-minimal
```

---

## D. Feedback / Temporal

### 11. feedback_pingpong
```bash
cargo run -p scheng-example-feedback-pingpong
```

### 12. feedback_orb
```bash
cargo run -p scheng-example-feedback-orb
```

### 13. temporal_slitscan
```bash
cargo run -p scheng-example-temporal-slitscan
```

---

## E. Readback

### 14. readback_minimal
```bash
cargo run -p scheng-example-readback-minimal
```

---

## F. OSC-Controlled

### 15. osc_minimal
```bash
cargo run -p scheng-example-osc-minimal
```

---

## G. Syphon Output (macOS)

### 16. syphon_minimal
```bash
cargo run -p scheng-example-syphon-minimal
```

### 17. syphon_patchbay
```bash
cargo run -p scheng-example-syphon-patchbay
```

### 18. syphon_builtin_legacy
```bash
cargo run -p scheng-example-syphon-builtin-legacy
```

---

## H. Video Decode / Capture

### 19. video_decode_source_minimal
```bash
cargo run -p scheng-example-video-decode-source-minimal -- \
  examples/video_decode_source_minimal/video_config.json
```

### 20. video_scrub_keyboard_transport
```bash
cargo run -p scheng-example-video-scrub-keyboard-transport -- \
  examples/video_scrub_keyboard_transport/video_config.json
```

### 21. video_source_minimal
```bash
cargo run -p scheng-example-video-source-minimal
```

### 22. video_device_capture_macos
```bash
cargo run -p scheng-example-video-device-capture-macos
```

### 23. webcam_source_minimal
```bash
cargo run -p scheng-example-webcam-source-minimal
```

---

## Notes

- Video examples require **ffmpeg** to be installed and reachable.
- Syphon examples require **macOS** with Syphon.framework.
- OSC examples require an OSC sender (e.g. TouchOSC, Max, PureData).
- Some examples may require camera permissions on first run.

This file is intended as a **runbook** to quickly explore the full scheng surface area.
