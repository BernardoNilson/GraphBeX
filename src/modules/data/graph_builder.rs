use std::sync::Arc;

use super::vertex_attributes::{AttributeCollector, VertexAttributes};
use super::vertex_ids::VertexIds;
use crate::modules::common::csr_graph::{UnweightedCsrGraph, WeightedCsrGraph};
use crate::modules::common::graph::{DEFAULT_EDGE_WEIGHT, Graph};

/// Where every format meets: the edges of a file, each given once, and the
/// vertices it named. Weights are only kept once an edge weighs something
/// other than the default, so an unweighted file builds the lighter graph.
#[derive(Debug, Default)]
pub struct GraphBuilder {
    edges: Vec<(u32, u32)>,
    /// Aligned with `edges`. `None` while every edge weighs the default.
    weights: Option<Vec<f32>>,
    /// The vertices of the formats that name them, including those without edges.
    names: VertexIds,
    attributes: AttributeCollector,
}

impl GraphBuilder {
    pub fn add_edge(&mut self, source: u32, target: u32, weight: f32) {
        self.edges.push((source, target));

        match &mut self.weights {
            Some(weights) => weights.push(weight),
            // The first edge with a weight of its own: the ones before it
            // weighed the default.
            None if weight != DEFAULT_EDGE_WEIGHT => {
                let mut weights = vec![DEFAULT_EDGE_WEIGHT; self.edges.len() - 1];
                weights.push(weight);
                self.weights = Some(weights);
            }
            None => (),
        }
    }

    /// The index of a vertex named in the file, given on first sight.
    pub fn vertex(&mut self, name: &[u8]) -> u32 {
        self.names.index_of(name)
    }

    pub fn set_attribute(&mut self, vertex: u32, name: &[u8], value: &[u8]) {
        self.attributes.set(vertex, name, value);
    }

    pub fn is_weighted(&self) -> bool {
        self.weights.is_some()
    }

    pub fn build(self) -> (Arc<dyn Graph>, VertexAttributes) {
        let edges = self.edges.into_iter();
        let named = self.names.len();

        let graph: Arc<dyn Graph> = match self.weights {
            None => Arc::new(UnweightedCsrGraph::from_edges(
                edges.map(|(source, target)| (source, target, ())),
                named,
            )),
            Some(weights) => Arc::new(WeightedCsrGraph::from_edges(
                edges.zip(weights).map(|((source, target), weight)| (source, target, weight)),
                named,
            )),
        };

        let attributes = self.attributes.finish(graph.node_count());
        (graph, attributes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_with_the_default_weight_build_an_unweighted_graph() {
        let mut builder = GraphBuilder::default();
        builder.add_edge(0, 1, DEFAULT_EDGE_WEIGHT);
        builder.add_edge(1, 2, DEFAULT_EDGE_WEIGHT);

        assert!(!builder.is_weighted());

        let (graph, _) = builder.build();
        assert_eq!(graph.edge_count(), 2);
        assert_eq!(graph.neighbor_weights(1), None);
    }

    #[test]
    fn a_single_weight_keeps_the_weights_of_every_edge() {
        let mut builder = GraphBuilder::default();
        builder.add_edge(0, 1, DEFAULT_EDGE_WEIGHT);
        builder.add_edge(1, 2, 2.5);
        builder.add_edge(2, 3, DEFAULT_EDGE_WEIGHT);

        assert!(builder.is_weighted());

        let (graph, _) = builder.build();
        assert_eq!(graph.neighbor_weights(1), Some(&[1.0, 2.5][..]));
        assert_eq!(graph.neighbor_weights(2), Some(&[2.5, 1.0][..]));
    }

    #[test]
    fn named_vertices_without_edges_are_kept() {
        let mut builder = GraphBuilder::default();
        let a = builder.vertex(b"a");
        let b = builder.vertex(b"b");
        builder.vertex(b"alone");
        builder.add_edge(a, b, DEFAULT_EDGE_WEIGHT);

        assert_eq!(builder.vertex(b"a"), a);
        assert_eq!(builder.build().0.node_count(), 3);
    }
}
