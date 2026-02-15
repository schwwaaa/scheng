## Step 8 — Output Surface Matrix Notes

**Surface:** CPU memory (readback)  
**Sink:** `ReadbackSink`

Pixel flow:

```
ShaderPass → PixelsOut → ReadbackSink → CPU buffer
```

This example is the foundation for recording/export pipelines (images/video).
It is intentionally explicit and deterministic, even if slower than real-time GPU sharing.
