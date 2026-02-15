## Step 8 — Output Surface Matrix Notes

**Surface:** Syphon  
**Sink:** `SyphonSink`

Pixel flow:

```
ShaderPass → PixelsOut → SyphonSink
```

Use this template when you want the lowest-latency **single-consumer** macOS GPU feed
(e.g., OBS / VJ tools).

For multi-output fan-out, see `syphon_patchbay`.
For CPU capture/recording, see `readback_minimal`.
