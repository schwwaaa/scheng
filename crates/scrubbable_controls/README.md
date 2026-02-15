# Scrubbable Controls Example (Keyboard + OSC, JSON driven)

This is a **self-contained control layer** you can drop next to scheng and
wire into your own examples (video source, decoder path, etc.).

It gives you:

- A `ControlLayer` struct that holds:
  - `TransportState` (position, speed, loop start/end, loop enable)
  - `ColorParams` (brightness, contrast, saturation)
  - `KeyMapConfig` for keyboard controls
  - `OscMapConfig` for OSC controls
- JSON-configurable key + OSC mappings so you can re-layout controls without
  recompiling.

## Files

- `src/lib.rs` – Rust control layer types + JSON loaders + apply logic.
- `keymap.json` – Example keyboard layout (string key IDs -> actions).
- `osc_map.json` – Example OSC address map (address -> binding).

## How to integrate

1. Copy this folder into your workspace (or turn it into a crate).
2. In your host / example:

   ```rust
   use scrubbable_controls_example::{
       ControlLayer, OscMessage,
   };

   // Load from wherever you want:
   let mut controls = ControlLayer::load(
       "path/to/keymap.json",
       "path/to/osc_map.json",
   )?;
   ```

3. On keyboard input (from winit or your host), convert the key into a string
   ID like `"space"`, `"left"`, `"k"` and call:

   ```rust
   controls.on_key("space");
   println!("transport = {:?}", controls.transport);
   println!("color     = {:?}", controls.color);
   ```

4. On OSC input (from `rosc` or your OSC receiver), build an `OscMessage`:

   ```rust
   let msg = OscMessage {
       addr: "/scheng/transport/speed".to_string(),
       args: vec![0.5], // half speed
   };
   controls.on_osc(msg);
   ```

5. In your render loop / engine, use:

   - `controls.transport.norm_pos` and `controls.transport.speed`
     to drive your scrubbable video node.
   - `controls.transport.loop_start`, `loop_end`, `loop_enabled`
     to clamp / wrap playback.
   - `controls.color.*` to set uniforms on your color-correction shader.

This keeps the **control plane** completely separate from the **engine /
graph**, and gives you the JSON-based customization you asked for.