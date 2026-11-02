use bevy::math::Vec3;
use rand::rngs::Xoshiro256PlusPlus;
use rand::{RngExt, SeedableRng};

use super::graph::Graph;
use super::layout_algorithm::LayoutAlgorithm;

/// Places every vertex uniformly in `[-bounds, bounds]` on each axis, in one
/// step. The same seed gives the same layout.
pub struct RandomLayout {
    dims: usize,
    bounds: f32,
    seed: u64,
    converged: bool,
}

impl RandomLayout {
    pub fn new(dims: usize, bounds: f32, seed: u64) -> Self {
        assert!(dims == 2 || dims == 3, "RandomLayout only supports 2D or 3D layouts.");
        Self { dims, bounds, seed, converged: false }
    }
}

impl LayoutAlgorithm for RandomLayout {
    fn name(&self) -> &str {
        "Random Layout"
    }

    fn step(&mut self, graph: &dyn Graph, positions: &mut [Vec3]) -> bool {
        if !self.converged {
            let mut rng = Xoshiro256PlusPlus::seed_from_u64(self.seed);

            for position in &mut positions[..graph.node_count()] {
                *position = Vec3::ZERO;
                for axis in 0..self.dims {
                    position[axis] = rng.random_range(-self.bounds..=self.bounds);
                }
            }
            self.converged = true;
        }
        false
    }

    fn is_converged(&self) -> bool {
        self.converged
    }
}
