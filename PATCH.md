# Workspace Cargo.toml patch

In `schengine/Cargo.toml`, update workspace members:

- remove old / broken member(s):
  - `examples/syphon_builtin` (and/or `examples/syphon_builtin_legacy`)

- add:
  - `examples/syphon_minimal`
  - `examples/syphon_patchbay`

Then run:

- `cargo test --workspace`
- `cargo run -p scheng-example-syphon-minimal`
- `cargo run -p scheng-example-syphon-patchbay`
