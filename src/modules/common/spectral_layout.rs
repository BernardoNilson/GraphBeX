use bevy::math::Vec3;
use sprs::{CsMat, TriMat};

use super::graph::{DEFAULT_EDGE_WEIGHT, Graph};
use super::layout_algorithm::LayoutAlgorithm;

const DEFAULT_TOLERANCE: f64 = 1e-9;
const DEFAULT_MAX_ITERATIONS: usize = 2_000;

/// Lays the graph out with the eigenvectors of its Laplacian `L = D - A`:
/// those of the `d` smallest nonzero eigenvalues are the coordinates. It is
/// deterministic and computed in one step.
pub struct SpectralLayout {
    dimensions: usize,
    /// Half the width of the cube the layout is scaled into.
    bounds: f32,
    /// The most power iterations spent on each eigenvector.
    max_iterations: usize,
    /// How close two iterations must be to stop early.
    tolerance: f64,
    converged: bool,
}

impl SpectralLayout {
    pub fn new(dimensions: usize, bounds: f32) -> Self {
        assert!(dimensions == 2 || dimensions == 3, "SpectralLayout only supports 2D or 3D layouts.");
        Self {
            dimensions,
            bounds,
            max_iterations: DEFAULT_MAX_ITERATIONS,
            tolerance: DEFAULT_TOLERANCE,
            converged: false,
        }
    }

    pub fn with_solver(mut self, max_iterations: usize, tolerance: f64) -> Self {
        self.max_iterations = max_iterations.max(1);
        self.tolerance = tolerance;
        self
    }

    /// The eigenvectors of the smallest nonzero eigenvalues, by power
    /// iteration on `shift·I − L`, whose largest eigenvalues are the smallest
    /// of `L`. Each iterate is kept orthogonal to the constant vector (the
    /// zero eigenvalue) and to the vectors found before.
    fn lowest_eigenvectors(&self, laplacian: &CsMat<f64>, shift: f64) -> Vec<Vec<f64>> {
        let n = laplacian.rows();
        let mut found = vec![vec![1.0 / (n as f64).sqrt(); n]];
        let mut next = vec![0.0; n];

        for axis in 0..self.dimensions {
            let mut vector = starting_vector(n, axis);

            if orthonormalize(&mut vector, &found) {
                for _ in 0..self.max_iterations {
                    // The sparse product accumulates, so `next` starts at zero.
                    next.fill(0.0);
                    sprs::prod::mul_acc_mat_vec_csr(laplacian.view(), &vector[..], &mut next[..]);
                    next.iter_mut().zip(&vector).for_each(|(out, v)| *out = shift * v - *out);

                    if !orthonormalize(&mut next, &found) {
                        break;
                    }

                    // Settled once the direction stops changing. The sign may flip.
                    let alignment = dot(&next, &vector);
                    std::mem::swap(&mut vector, &mut next);
                    if 1.0 - alignment.abs() < self.tolerance {
                        break;
                    }
                }
            } else {
                vector.fill(0.0);
            }

            found.push(vector);
        }

        found.split_off(1)
    }
}

impl LayoutAlgorithm for SpectralLayout {
    fn name(&self) -> &str {
        "Spectral Layout"
    }

    fn step(&mut self, graph: &dyn Graph, positions: &mut [Vec3]) -> bool {
        if !self.converged && graph.node_count() > 0 {
            let (laplacian, shift) = laplacian(graph);
            let axes = self.lowest_eigenvectors(&laplacian, shift);
            write_positions(&axes, self.bounds, positions);
        }

        self.converged = true;
        false
    }

    fn is_converged(&self) -> bool {
        self.converged
    }
}

/// The weighted Laplacian, and twice its largest degree, which bounds its
/// spectrum (Gershgorin's circles). Self-loops are skipped: they would cancel
/// out in `D - A`.
fn laplacian(graph: &dyn Graph) -> (CsMat<f64>, f64) {
    let n = graph.node_count();
    let mut triplets = TriMat::new((n, n));
    let mut largest: f64 = 1.0;

    for vertex in 0..n as u32 {
        let weights = graph.neighbor_weights(vertex);
        let mut degree = 0.0;

        for (index, &neighbor) in graph.neighbors(vertex).iter().enumerate() {
            if neighbor != vertex {
                let weight = f64::from(weights.map_or(DEFAULT_EDGE_WEIGHT, |weights| weights[index]));
                triplets.add_triplet(vertex as usize, neighbor as usize, -weight);
                degree += weight;
            }
        }

        triplets.add_triplet(vertex as usize, vertex as usize, degree);
        largest = largest.max(degree);
    }

    (triplets.to_csr(), 2.0 * largest)
}

/// A fixed starting vector, so the same graph always gives the same layout.
/// The golden ratio spreads the values without repeating patterns.
fn starting_vector(n: usize, axis: usize) -> Vec<f64> {
    const GOLDEN: f64 = 0.618_033_988_749_894_8;
    (0..n).map(|index| ((index + 1 + axis * n) as f64 * GOLDEN).fract() - 0.5).collect()
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}

/// Removes the components of `vector` along each (unit) basis vector and
/// scales it to unit length. False if nothing is left.
fn orthonormalize(vector: &mut [f64], basis: &[Vec<f64>]) -> bool {
    for base in basis {
        let projection = dot(vector, base);
        vector.iter_mut().zip(base).for_each(|(value, base)| *value -= projection * base);
    }

    let norm = dot(vector, vector).sqrt();
    if norm <= f64::EPSILON {
        return false;
    }
    vector.iter_mut().for_each(|value| *value /= norm);
    true
}

/// The eigenvector components as coordinates. Unit vectors shrink as the
/// graph grows, so every axis is scaled by one factor until the largest
/// component reaches the bounds, which keeps the proportions.
fn write_positions(axes: &[Vec<f64>], bounds: f32, positions: &mut [Vec3]) {
    let largest = axes.iter().flatten().fold(0.0f64, |largest, value| largest.max(value.abs()));
    let scale = if largest <= f64::EPSILON { 0.0 } else { f64::from(bounds) / largest };

    for (vertex, position) in positions.iter_mut().enumerate() {
        *position = Vec3::ZERO;
        for (axis, values) in axes.iter().enumerate() {
            position[axis] = (values[vertex] * scale) as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::common::csr_graph::UnweightedCsrGraph;

    fn graph(vertices: usize, edges: &[(u32, u32)]) -> UnweightedCsrGraph {
        UnweightedCsrGraph::from_edges(edges.iter().map(|&(u, v)| (u, v, ())), vertices)
    }

    fn path(vertices: u32) -> UnweightedCsrGraph {
        let edges: Vec<_> = (0..vertices - 1).map(|v| (v, v + 1)).collect();
        graph(vertices as usize, &edges)
    }

    fn layout(graph: &UnweightedCsrGraph, dimensions: usize) -> Vec<Vec3> {
        let mut positions = vec![Vec3::ZERO; graph.node_count()];
        let mut algorithm = SpectralLayout::new(dimensions, 100.0);

        assert!(!algorithm.step(graph, &mut positions));
        assert!(algorithm.is_converged());
        positions
    }

    #[test]
    fn orders_the_vertices_of_a_path_along_the_first_axis() {
        // The Fiedler vector of a path grows from one end to the other.
        let first: Vec<f32> = layout(&path(8), 3).iter().map(|position| position.x).collect();

        let increasing = first.windows(2).all(|pair| pair[0] < pair[1]);
        let decreasing = first.windows(2).all(|pair| pair[0] > pair[1]);
        assert!(increasing || decreasing, "the path should be laid out in order, got {first:?}");
    }

    #[test]
    fn separates_the_two_sides_of_a_barbell() {
        // Two triangles joined by one edge: the Fiedler vector takes one sign
        // on each side.
        let positions = layout(&graph(6, &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (2, 3)]), 3);
        let left = positions[0].x + positions[1].x + positions[2].x;
        let right = positions[3].x + positions[4].x + positions[5].x;

        assert!(left * right < 0.0, "the sides should be on opposite sides of the origin: {left}, {right}");
    }

    #[test]
    fn is_deterministic_bounded_and_flat_in_2d() {
        assert_eq!(layout(&path(6), 3), layout(&path(6), 3));
        assert!(layout(&path(10), 3).iter().all(|position| position.abs().max_element() <= 100.0));
        assert!(layout(&path(6), 2).iter().all(|position| position.z == 0.0));
    }

    #[test]
    fn handles_a_graph_without_edges_and_a_second_step() {
        assert_eq!(layout(&graph(3, &[]), 3).len(), 3);

        let graph = path(5);
        let mut positions = vec![Vec3::ZERO; 5];
        let mut algorithm = SpectralLayout::new(3, 100.0);
        algorithm.step(&graph, &mut positions);
        let first = positions.clone();

        assert!(!algorithm.step(&graph, &mut positions));
        assert_eq!(first, positions);
    }
}
