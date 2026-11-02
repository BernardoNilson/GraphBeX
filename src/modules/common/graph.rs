/// What an edge weighs when the graph stores no weight for it.
pub const DEFAULT_EDGE_WEIGHT: f32 = 1.0;

/// The contract of every graph representation. Vertices are `u32`, as in the
/// memory analysis of the TCC. `Send + Sync` lets the layout thread and the
/// rendering share one graph through an `Arc`.
pub trait Graph: Send + Sync {
    fn node_count(&self) -> usize;

    /// Distinct edges: an undirected edge counts once, though stored twice.
    fn edge_count(&self) -> usize;

    /// The neighbors in ascending order, borrowed without copying.
    fn neighbors(&self, node: u32) -> &[u32];

    /// The edge weights, aligned with `neighbors`. `None` when the graph
    /// stores none and every edge weighs `DEFAULT_EDGE_WEIGHT`.
    fn neighbor_weights(&self, node: u32) -> Option<&[f32]>;

    fn degree(&self, node: u32) -> usize;

    fn has_edge(&self, u: u32, v: u32) -> bool;
}
