//! Compile-only compatibility crate.
//!
//! This crate exists to ensure the public SDK surface remains usable by third-party
//! consumers. It is not shipped or run; it must only build.

use scheng_graph::{Graph, NodeKind};
use scheng_runtime::{standard_op_for, BankDef, BankSet, MatrixPreset, SceneDef};

#[allow(dead_code)]
pub fn _compile_witness() {
    // Graph builds and compiles using only public APIs.
    let mut g = Graph::new();

    // Minimal shader source + pass + output chain (kinds exist in graph).
    let src = g.add_node(NodeKind::ShaderSource);
    let pass = g.add_node(NodeKind::ShaderPass);
    let out = g.add_node(NodeKind::PixelsOut);

    // Ports are string-addressed in the graph; this is intentionally minimal.
    // The compat crate only verifies that compile-time wiring APIs exist.
    let _ = (src, pass, out);

    // Standard runtime mapping must remain callable.
    let _op = standard_op_for(NodeKind::ShaderPass);

    // Runtime data models must remain constructible using stable, backend-agnostic APIs.
    // Avoid `Default` here: the SDK surface may prefer explicit constructors.
    let _banks = BankSet::builtin_matrix_banks();
    let _scene = SceneDef {
        name: "solo_0".to_string(),
        preset: MatrixPreset::Solo0,
    };
    let _bank = BankDef {
        name: "Basic".to_string(),
        scenes: vec![_scene],
    };
    let _ = (_banks, _bank);
}
