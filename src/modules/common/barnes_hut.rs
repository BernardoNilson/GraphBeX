//! The spatial tree of the Barnes-Hut approximation: one tree whose cells have
//! 2^d children, a quadtree in 2D and an octree in 3D.

use std::ops::Range;

use bevy::math::Vec3;

/// Stops subdividing vertices that sit on the same point.
const MAX_DEPTH: usize = 32;

/// Closer than this, two bodies are the same point.
const MIN_DISTANCE: f32 = 1e-6;

#[derive(Clone)]
struct Cell {
    centre_of_mass: Vec3,
    /// The number of vertices in the cell.
    mass: f32,
    width: f32,
    /// The children in the arena, empty for a leaf.
    children: Range<u32>,
}

const EMPTY: Cell = Cell { centre_of_mass: Vec3::ZERO, mass: 0.0, width: 0.0, children: 0..0 };

pub struct SpatialTree {
    cells: Vec<Cell>,
    dimensions: usize,
}

impl SpatialTree {
    pub fn build(positions: &[Vec3], dimensions: usize) -> Self {
        let mut tree = Self { cells: Vec::with_capacity(positions.len() * 2), dimensions };
        if positions.is_empty() {
            return tree;
        }

        let (min, max) = positions
            .iter()
            .fold((Vec3::MAX, Vec3::MIN), |(min, max), p| (min.min(*p), max.max(*p)));
        let half = ((max - min).max_element() / 2.0).max(MIN_DISTANCE);

        let mut indices: Vec<u32> = (0..positions.len() as u32).collect();
        tree.cells.push(EMPTY);
        tree.fill(0, positions, &mut indices, (min + max) / 2.0, half, 0);
        tree
    }

    fn fill(&mut self, slot: usize, positions: &[Vec3], indices: &mut [u32], centre: Vec3, half: f32, depth: usize) {
        let sum: Vec3 = indices.iter().map(|&i| positions[i as usize]).sum();
        let mass = indices.len() as f32;
        self.cells[slot] = Cell { centre_of_mass: sum / mass, mass, width: 2.0 * half, children: 0..0 };

        if indices.len() <= 1 || depth >= MAX_DEPTH {
            return;
        }

        // Sorting by quadrant leaves the bodies of each child contiguous. Only
        // the quadrants that hold a body become children.
        let quadrant = |i: &u32| self.quadrant(positions[*i as usize], centre);
        indices.sort_unstable_by_key(quadrant);

        let mut groups = Vec::new();
        let mut start = 0;
        while start < indices.len() {
            let q = quadrant(&indices[start]);
            let end = start + indices[start..].iter().take_while(|i| quadrant(i) == q).count();
            groups.push((q, start..end));
            start = end;
        }

        let first = self.cells.len() as u32;
        self.cells.extend(std::iter::repeat_n(EMPTY, groups.len()));
        self.cells[slot].children = first..first + groups.len() as u32;

        for (offset, (q, range)) in groups.into_iter().enumerate() {
            let child_centre = centre + self.offset(q) * (half / 2.0);
            self.fill(first as usize + offset, positions, &mut indices[range], child_centre, half / 2.0, depth + 1);
        }
    }

    /// The quadrant (or octant) of a position: one bit per axis.
    fn quadrant(&self, position: Vec3, centre: Vec3) -> usize {
        (0..self.dimensions).filter(|&axis| position[axis] >= centre[axis]).map(|axis| 1 << axis).sum()
    }

    fn offset(&self, quadrant: usize) -> Vec3 {
        let mut offset = Vec3::ZERO;
        for axis in 0..self.dimensions {
            offset[axis] = if quadrant & (1 << axis) != 0 { 1.0 } else { -1.0 };
        }
        offset
    }

    /// The repulsion on a body at `position`: `k^2 * mass / d` from each cell
    /// that is a leaf or looks small enough, `width / d < theta`.
    pub fn repulsion(&self, position: Vec3, k_squared: f32, theta: f32) -> Vec3 {
        let mut force = Vec3::ZERO;
        if !self.cells.is_empty() {
            self.visit(0, position, k_squared, theta, &mut force);
        }
        force
    }

    fn visit(&self, index: usize, position: Vec3, k_squared: f32, theta: f32, force: &mut Vec3) {
        let cell = &self.cells[index];
        let delta = position - cell.centre_of_mass;
        let distance = delta.length();
        let leaf = cell.children.is_empty();

        if leaf && distance < MIN_DISTANCE {
            return; // the body itself
        }

        if leaf || cell.width / distance < theta {
            *force += delta / distance * (k_squared * cell.mass / distance);
        } else {
            for child in cell.children.clone() {
                self.visit(child as usize, position, k_squared, theta, force);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn points() -> Vec<Vec3> {
        (0..50)
            .map(|i| {
                let i = i as f32;
                Vec3::new((i * 7.3) % 13.0, (i * 3.1) % 11.0, (i * 5.7) % 9.0)
            })
            .collect()
    }

    /// The all-pairs repulsion Barnes-Hut approximates.
    fn exact(positions: &[Vec3], index: usize, k_squared: f32) -> Vec3 {
        let others = positions.iter().enumerate().filter(|&(other, _)| other != index);
        others
            .map(|(_, p)| {
                let delta = positions[index] - *p;
                delta / delta.length() * (k_squared / delta.length())
            })
            .sum()
    }

    #[test]
    fn the_root_holds_every_body_at_their_mean() {
        let positions = points();
        let tree = SpatialTree::build(&positions, 3);
        let mean = positions.iter().copied().sum::<Vec3>() / positions.len() as f32;

        assert_eq!(tree.cells[0].mass, 50.0);
        assert!(tree.cells[0].centre_of_mass.abs_diff_eq(mean, 1e-4));
    }

    #[test]
    fn theta_zero_is_exact_and_a_usual_theta_is_close() {
        let positions = points();
        let tree = SpatialTree::build(&positions, 3);

        for index in [0, 17, 49] {
            assert!(tree.repulsion(positions[index], 4.0, 0.0).abs_diff_eq(exact(&positions, index, 4.0), 1e-3));
        }

        let exact = exact(&positions, 5, 4.0);
        assert!((tree.repulsion(positions[5], 4.0, 0.8) - exact).length() < 0.1 * exact.length());
    }

    #[test]
    fn the_2d_tree_splits_in_four() {
        let positions: Vec<Vec3> = points().iter().map(|p| p.with_z(0.0)).collect();

        assert!(SpatialTree::build(&positions, 2).cells[0].children.len() <= 4);
    }

    #[test]
    fn coincident_bodies_do_not_recurse_forever() {
        let tree = SpatialTree::build(&[Vec3::ONE; 10], 3);

        assert_eq!(tree.repulsion(Vec3::ONE, 1.0, 0.8), Vec3::ZERO);
    }
}
