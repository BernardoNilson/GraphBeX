use bevy::math::Vec3;
use rayon::prelude::*;

use super::barnes_hut::SpatialTree;
use super::graph::Graph;
use super::layout_algorithm::LayoutAlgorithm;
use super::random_layout::RandomLayout;

/// The constant `C` of the ideal distance `k = C * (A / |V|)^(1/d)`.
const IDEAL_DISTANCE_CONSTANT: f32 = 1.0;

/// The starting temperature as a fraction of the frame width (W/10, as in the
/// original paper).
const INITIAL_TEMPERATURE_RATIO: f32 = 0.1;

/// Fruchterman-Reingold with Barnes-Hut repulsion, in 2D or 3D. It runs a
/// fixed number of iterations, cooling linearly to zero.
pub struct FruchtermanReingold {
    dimensions: usize,
    bounds: f32,
    seed: u64,
    iterations: usize,
    theta: f32,
    iteration: usize,
    temperature: f32,
}

impl FruchtermanReingold {
    pub fn new(dimensions: usize, bounds: f32, seed: u64, iterations: usize) -> Self {
        assert!(dimensions == 2 || dimensions == 3, "FruchtermanReingold only supports 2D or 3D layouts.");
        Self { dimensions, bounds, seed, iterations: iterations.max(1), theta: 0.8, iteration: 0, temperature: 0.0 }
    }

    pub fn with_barnes_hut(mut self, theta: f32) -> Self {
        self.theta = theta;
        self
    }

    fn frame_width(&self) -> f32 {
        2.0 * self.bounds
    }
}

impl LayoutAlgorithm for FruchtermanReingold {
    fn name(&self) -> &str {
        "Fruchterman-Reingold"
    }

    /// One iteration. The first places the vertices with the random layout.
    fn step(&mut self, graph: &dyn Graph, positions: &mut [Vec3]) -> bool {
        if self.is_converged() {
            return false;
        }
        if self.iteration == 0 {
            RandomLayout::new(self.dimensions, self.bounds, self.seed).step(graph, positions);
            self.temperature = INITIAL_TEMPERATURE_RATIO * self.frame_width();
        }

        let vertices = graph.node_count().max(1) as f32;
        let k = IDEAL_DISTANCE_CONSTANT * self.frame_width() / vertices.powf(1.0 / self.dimensions as f32);

        // The tree is built sequentially and the forces in parallel.
        // Each vertex sums the attraction of all its neighbors, since the CSR
        // stores every edge both ways, so no vertex writes another's force.
        let tree = SpatialTree::build(positions, self.dimensions);
        let displacements: Vec<Vec3> = (0..positions.len())
            .into_par_iter()
            .map(|vertex| {
                let position = positions[vertex];
                // f_a(d) = d^2 / k along the edge.
                let attraction: Vec3 = graph
                    .neighbors(vertex as u32)
                    .iter()
                    .map(|&neighbor| {
                        let delta = position - positions[neighbor as usize];
                        delta * delta.length() / k
                    })
                    .sum();
                tree.repulsion(position, k * k, self.theta) - attraction
            })
            .collect();

        let temperature = self.temperature;
        positions.par_iter_mut().zip(displacements).for_each(|(position, displacement)| {
            let length = displacement.length();
            if length > 0.0 {
                *position += displacement / length * length.min(temperature);
            }
        });

        self.iteration += 1;
        self.temperature = INITIAL_TEMPERATURE_RATIO
            * self.frame_width()
            * (1.0 - self.iteration as f32 / self.iterations as f32);

        !self.is_converged()
    }

    fn is_converged(&self) -> bool {
        self.iteration >= self.iterations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::common::csr_graph::UnweightedCsrGraph;

    fn graph(vertices: usize, edges: &[(u32, u32)]) -> UnweightedCsrGraph {
        UnweightedCsrGraph::from_edges(edges.iter().map(|&(u, v)| (u, v, ())), vertices)
    }

    fn run(graph: &UnweightedCsrGraph, dimensions: usize, seed: u64) -> Vec<Vec3> {
        let mut layout = FruchtermanReingold::new(dimensions, 50.0, seed, 200);
        let mut positions = vec![Vec3::ZERO; graph.node_count()];
        while layout.step(graph, &mut positions) {}
        positions
    }

    #[test]
    fn stops_after_the_iteration_limit() {
        let graph = graph(3, &[(0, 1), (1, 2)]);
        let mut layout = FruchtermanReingold::new(3, 50.0, 1, 5);
        let mut positions = vec![Vec3::ZERO; 3];

        assert_eq!((0..10).take_while(|_| layout.step(&graph, &mut positions)).count(), 4);
        assert!(layout.is_converged());
        assert!(!layout.step(&graph, &mut positions));
    }

    #[test]
    fn connected_vertices_end_closer_than_separate_components() {
        let positions = run(&graph(6, &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3)]), 3, 7);
        let (within, across) = (positions[0].distance(positions[1]), positions[0].distance(positions[4]));

        assert!(within < across, "within {within}, across {across}");
    }

    #[test]
    fn is_flat_in_2d_and_reproducible_with_the_same_seed() {
        let path = graph(5, &[(0, 1), (1, 2), (2, 3), (3, 4)]);

        assert!(run(&path, 2, 3).iter().all(|position| position.z == 0.0));
        assert_eq!(run(&path, 3, 11), run(&path, 3, 11));
    }
}
