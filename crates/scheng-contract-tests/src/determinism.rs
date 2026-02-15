#![forbid(unsafe_code)]

#[cfg(test)]
mod tests {
    use scheng_graph::{Graph, NodeKind};

    /// Determinism contract:
    /// compiling the same graph twice yields the same Plan node ordering.
    #[test]
    fn graph_compile_is_deterministic_for_same_graph() {
        let mut g = Graph::new();

        let src = g.add_node(NodeKind::ShaderSource);
        let pass = g.add_node(NodeKind::ShaderPass);
        let out = g.add_node(NodeKind::PixelsOut);

        // Use the graph's public helper for name-based wiring.
        g.connect_named(src, "out", pass, "in")
            .expect("connect_named src.out -> pass.in");
        g.connect_named(pass, "out", out, "in")
            .expect("connect_named pass.out -> out.in");

        let p1 = g.compile().expect("compile 1");
        let p2 = g.compile().expect("compile 2");

        assert_eq!(p1.nodes, p2.nodes, "plan nodes order must be stable");
        assert_eq!(p1.edges.len(), p2.edges.len(), "edge count must be stable");
    }
}
