use bevy::math::Vec3;

use super::graph::Graph;

/// A spatial distribution algorithm. `Send` because it runs on the thread of
/// its layout run.
pub trait LayoutAlgorithm: Send {
    fn name(&self) -> &str;

    /// One iteration over the positions, indexed by vertex. Returns whether
    /// more iterations are needed.
    fn step(&mut self, graph: &dyn Graph, positions: &mut [Vec3]) -> bool;

    fn is_converged(&self) -> bool;
}
