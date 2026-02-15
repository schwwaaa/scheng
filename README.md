# scheng SDK (v0.1.0) (WIP)

This workspace is the developer-facing SDK extracted from scheng.

- `scheng-core`: pure data model (configs/events), no OS/GL/IO deps.
- `scheng-runtime-glow`: OpenGL/glow runtime ("shader machine").
- `scheng-host-winit`: optional host glue (windowing/policy), kept separate.
- `examples/minimal`: shows intended SDK usage.

Design rule: **engine takes bytes/handles, returns pixels/errors; host handles files/windows/devices/policy.**

## Engine Contract

See `docs/ENGINE_CONTRACT.md`.

## Runtime invariants (read this if you are building on scheng)

- `docs/RUNTIME_INVARIANTS.md` — the non-negotiable layering rules
- `docs/PORTS_AND_MIXERS.md` — PortId ordering + mixer semantics
- `docs/SCENES_AND_BANKS.md` — banks/scenes data model + JSON schema

## Examples

```bash
cargo run -p scheng-example-minimal
cargo run -p scheng-example-pure-single-pass
cargo run -p scheng-example-render-target-only
```

Graph → Plan → Runtime (C3):

```bash
cargo run -p scheng-example-graph-minimal
cargo run -p scheng-example-graph-chain2
cargo run -p scheng-example-graph-mixer2
cargo run -p scheng-example-graph-mixer-builtin
```

Matrix mixer + banks/scenes (C4):

```bash
cargo run -p scheng-example-graph-matrix-mix4 -- --banks banks.json
```

```bash
cargo run -p scheng-example-feedback-pingpong
```

## OSC (example)

`feedback_pingpong` listens on `127.0.0.1:9000` for `/param/<name> <float>` messages.

Params: `u_speed`, `u_amount`, `u_shift`, `u_mix`.

```bash
cargo run -p scheng-example-feedback-orb
```

`feedback_orb` params: `u_decay`, `u_gain`, `u_blur`, `u_add`, `u_smear`, `u_smear_angle`, `u_smear_strength`, `u_mix`, `u_bg`.

Max patches are in `max/`.


## Contract Gate (forbidden deps)

Run locally:

```bash
bash ci/check_forbidden_deps.sh
```
