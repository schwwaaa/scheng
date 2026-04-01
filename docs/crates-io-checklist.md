# scheng — crates.io Publishing Checklist

## What publishing means
Every crate in `/Users/tgm/Documents/SPLASH/scheng/crates/` becomes independently 
installable via `cargo add scheng-runtime-wgpu` etc. This is what enables 
third-party instrument authors to build on scheng without cloning the repo.

## Crates to publish (in dependency order)
Publish these first — they have no internal scheng dependencies:
1. `scheng-graph`
2. `scheng-param-store`
3. `scheng-hotreload`

Then publish these (depend on the above):
4. `scheng-runtime-wgpu`
5. `scheng-input-midi`
6. `scheng-control-osc-wgpu`
7. `scheng-output-syphon`
8. `scheng-output-ndi`
9. `scheng-output-ffmpeg`
10. `scheng-input-webcam`
11. `scheng-input-video`

## What needs to happen before publishing

### For each crate
- [ ] `Cargo.toml` has `description`, `license`, `repository`, `homepage`, `keywords`, `categories`
- [ ] `version = "0.1.0"` is consistent across all crates
- [ ] Path dependencies replaced with version constraints on published crates
- [ ] README.md at crate root (crates.io shows this)
- [ ] No `publish = false` in Cargo.toml

### Cargo.toml fields needed (example for scheng-graph)
```toml
[package]
name        = "scheng-graph"
version     = "0.1.0"
edition     = "2021"
description = "Directed acyclic graph for GPU shader pipeline composition"
license     = "MIT OR Apache-2.0"
repository  = "https://github.com/[org]/scheng"
homepage    = "https://scheng.dev"
keywords    = ["gpu", "shader", "video", "synthesis", "wgpu"]
categories  = ["multimedia", "graphics", "rendering"]
```

### Platform-gated crates (macOS only)
- `scheng-output-syphon` — mark with `[target.'cfg(target_os = "macos")'.dependencies]`
- Consider publishing as macOS-only with a clear README note

### The hard dependency: Syphon.framework
- `scheng-output-syphon` links to a vendored binary framework
- This CANNOT be published to crates.io in the standard way
- Options:
  a. Publish the crate, require users to vendor the framework themselves
  b. Document the download step in README
  c. Use a build.rs that downloads the framework at build time

### Minimum viable publish order
To get the most useful crates out first:
1. `scheng-graph` — pure Rust, no external deps, publish immediately
2. `scheng-param-store` — pure Rust, publish immediately
3. `scheng-hotreload` — pure Rust, publish immediately
4. `scheng-runtime-wgpu` — core engine, needs wgpu version pinning

## What NOT to publish yet
- `scheng-contract-tests` — internal test crate
- `scheng-example-instrument` — example, not a library
- Any crate with `publish = false`

## Steps to publish scheng-graph right now
```bash
cd /Users/tgm/Documents/SPLASH/scheng/crates/scheng-graph

# 1. Verify Cargo.toml has all required fields
cat Cargo.toml

# 2. Dry run — checks everything without publishing
cargo publish --dry-run

# 3. Publish
cargo publish

# 4. Wait 30 seconds, then publish next crate in order
```

## Decision needed from you
Before publishing, decide:
1. GitHub org name — crates.io and repository URL need this
2. License — MIT, Apache-2.0, or MIT OR Apache-2.0 (dual is most permissive/standard)
3. Homepage URL — scheng.dev or similar
4. Whether to publish Syphon crate now or defer until framework vendoring is solved
