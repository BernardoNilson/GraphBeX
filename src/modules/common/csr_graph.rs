use petgraph::Undirected;
use petgraph::csr::Csr;

use super::graph::Graph;

/// What a `CsrGraph` stores per edge. `f32` keeps the weight. `()` keeps
/// nothing, and a vector of `()` never allocates, which drops the optional
/// `weights` vector of the memory analysis.
pub trait EdgeWeight: Copy + Send + Sync + 'static {
    fn as_weights(stored: &[Self]) -> Option<&[f32]>;
}

impl EdgeWeight for f32 {
    fn as_weights(stored: &[f32]) -> Option<&[f32]> {
        Some(stored)
    }
}

impl EdgeWeight for () {
    fn as_weights(_: &[()]) -> Option<&[f32]> {
        None
    }
}

/// An adjacency entry: its row, its column and what the edge stores.
type Entry<W> = (u32, u32, W);

/// The Compressed Sparse Row graph, over petgraph's `Csr` with 32-bit
/// indices, as the implementation strategy of the TCC asks.
#[derive(Debug)]
pub struct CsrGraph<W: EdgeWeight = f32> {
    csr: Csr<(), W, Undirected, u32>,
    edge_count: usize,
}

pub type WeightedCsrGraph = CsrGraph<f32>;
pub type UnweightedCsrGraph = CsrGraph<()>;

impl<W: EdgeWeight> Graph for CsrGraph<W> {
    fn node_count(&self) -> usize {
        self.csr.node_count()
    }

    fn edge_count(&self) -> usize {
        self.edge_count
    }

    fn neighbors(&self, node: u32) -> &[u32] {
        self.csr.neighbors_slice(node)
    }

    fn neighbor_weights(&self, node: u32) -> Option<&[f32]> {
        W::as_weights(self.csr.edges_slice(node))
    }

    fn degree(&self, node: u32) -> usize {
        self.csr.out_degree(node)
    }

    fn has_edge(&self, u: u32, v: u32) -> bool {
        self.csr.contains_edge(u, v)
    }
}

impl<W: EdgeWeight> CsrGraph<W> {
    /// Builds the CSR from undirected edges, each given once, in O(|V| + |E|).
    /// `named_vertices` keeps vertices a file declares without edges.
    pub fn from_edges(edges: impl ExactSizeIterator<Item = Entry<W>>, named_vertices: usize) -> Self {
        // Each edge is stored in both directions, a self-loop only once.
        let mut entries = Vec::with_capacity(edges.len() * 2);
        let mut vertices = named_vertices;

        for (source, target, weight) in edges {
            vertices = vertices.max(source.max(target) as usize + 1);
            entries.push((source, target, weight));
            if source != target {
                entries.push((target, source, weight));
            }
        }

        // `from_sorted_edges` needs the entries sorted and without duplicates.
        // The sort is stable, so the duplicate kept is the file's first.
        let mut entries = sort_entries(entries, vertices);
        entries.dedup_by_key(|&mut (source, target, _)| (source, target));

        let edge_count = entries.iter().filter(|&&(source, target, _)| source <= target).count();
        let mut csr = Csr::from_sorted_edges(&entries).expect("the entries were just sorted and deduplicated");

        // The structure is sized by the highest vertex of an edge.
        while csr.node_count() < vertices {
            csr.add_node(());
        }

        Self { csr, edge_count }
    }
}

/// A radix sort by (source, target) in O(|V| + |E|): a stable counting sort by
/// target, then another by source.
fn sort_entries<W: Copy>(entries: Vec<Entry<W>>, vertices: usize) -> Vec<Entry<W>> {
    let by_target = counting_sort(&entries, vertices, |&(_, target, _)| target);
    drop(entries);
    counting_sort(&by_target, vertices, |&(source, _, _)| source)
}

/// A stable counting sort by a key below `buckets`.
fn counting_sort<W: Copy>(entries: &[Entry<W>], buckets: usize, key: impl Fn(&Entry<W>) -> u32) -> Vec<Entry<W>> {
    // Counts shifted by one, so the prefix sum gives where each bucket starts.
    let mut starts = vec![0usize; buckets + 1];
    for entry in entries {
        starts[key(entry) as usize + 1] += 1;
    }
    for bucket in 0..buckets {
        starts[bucket + 1] += starts[bucket];
    }

    let mut sorted = entries.to_vec();
    for entry in entries {
        let slot = &mut starts[key(entry) as usize];
        sorted[*slot] = *entry;
        *slot += 1;
    }
    sorted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weighted(edges: &[(u32, u32, f32)], named: usize) -> WeightedCsrGraph {
        CsrGraph::from_edges(edges.iter().copied(), named)
    }

    fn unweighted(edges: &[(u32, u32)]) -> UnweightedCsrGraph {
        CsrGraph::from_edges(edges.iter().map(|&(u, v)| (u, v, ())), 0)
    }

    #[test]
    fn builds_a_symmetric_csr_with_sorted_neighbors() {
        let graph = unweighted(&[(0, 1), (1, 2), (0, 2)]);
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.neighbors(1), &[0, 2]);
        assert_eq!(graph.degree(0), 2);
        assert!(graph.has_edge(0, 1) && graph.has_edge(1, 0));

        let shuffled = unweighted(&[(3, 0), (2, 4), (0, 4), (1, 0), (4, 1), (0, 2)]);
        assert_eq!(shuffled.neighbors(0), &[1, 2, 3, 4]);
        assert_eq!(shuffled.neighbors(4), &[0, 1, 2]);
    }

    #[test]
    fn counts_distinct_edges_and_self_loops_once() {
        assert_eq!(unweighted(&[(0, 1), (1, 2), (0, 2)]).edge_count(), 3);
        assert_eq!(unweighted(&[(0, 1), (2, 2)]).edge_count(), 2);
        assert_eq!(unweighted(&[(0, 0)]).neighbors(0), &[0]);
        assert_eq!(unweighted(&[]).node_count(), 0);
    }

    #[test]
    fn drops_duplicate_edges_keeping_the_first_weight() {
        let graph = weighted(&[(0, 1, 2.0), (1, 0, 3.0), (0, 1, 4.0)], 0);

        assert_eq!(graph.edge_count(), 1);
        assert_eq!(graph.neighbor_weights(0), Some(&[2.0][..]));
        assert_eq!(graph.neighbor_weights(1), Some(&[2.0][..]));
    }

    #[test]
    fn keeps_named_vertices_without_edges() {
        let graph = weighted(&[(0, 1, 1.0)], 4);

        assert_eq!(graph.node_count(), 4);
        assert!(graph.neighbors(3).is_empty());
    }

    #[test]
    fn only_a_weighted_graph_exposes_weights() {
        assert_eq!(weighted(&[(0, 1, 2.5), (1, 2, 1.0)], 0).neighbor_weights(1), Some(&[2.5, 1.0][..]));
        assert_eq!(unweighted(&[(0, 1)]).neighbor_weights(0), None);
    }
}
