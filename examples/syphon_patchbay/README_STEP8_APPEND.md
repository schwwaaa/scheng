## Step 8 — Output Surface Matrix Notes

`syphon_patchbay` demonstrates **fan-out routing**.

**Surfaces:**
- `preview` → Window (on-screen)
- `program` → Syphon

Routing:

```
ShaderPass → PixelsOut → PatchbaySink
                          ├─ preview (Window)
                          └─ program (SyphonSink)
```

Use this when you want **preview + program** (or any multi-output layout).
This pattern scales forward into recording/export by adding `ReadbackSink` as another route.
