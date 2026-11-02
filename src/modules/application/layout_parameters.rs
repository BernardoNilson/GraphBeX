//! The parameters the layouts expose to the user (FR2), described as data so
//! the interface builds their fields without knowing the algorithms.

use std::collections::HashMap;

/// The mean distance between vertices the automatic bounds aim for.
pub const VERTEX_SPACING: f64 = 20.0;

/// Keeps tiny graphs from being squeezed into a point.
const MIN_AUTOMATIC_BOUNDS: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParameterId {
    /// Half the width of the region the layout is placed in.
    Bounds,
    Seed,
    /// The most iterations the spectral eigensolver may spend.
    IterationLimit,
    /// How small a change must be for the eigensolver to stop.
    ConvergenceThreshold,
    /// How many iterations the force-directed layout runs.
    Iterations,
    Theta,
    SnapshotInterval,
}

/// How the − and + buttons move a value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stepping {
    Integer(f64),
    /// Multiplied or divided, for values spanning orders of magnitude.
    Factor(f64),
    Decimal(f64),
}

#[derive(Debug)]
pub struct ParameterSpec {
    pub label: &'static str,
    pub stepping: Stepping,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    /// Derives the value from the vertex count and dimensions. Such a
    /// parameter starts on `Auto`.
    pub automatic: Option<fn(usize, usize) -> f64>,
}

const fn spec(label: &'static str, stepping: Stepping, min: f64, max: f64, default: f64) -> ParameterSpec {
    ParameterSpec { label, stepping, min, max, default, automatic: None }
}

const BOUNDS: ParameterSpec = ParameterSpec {
    automatic: Some(automatic_bounds),
    ..spec("Bounds", Stepping::Factor(1.25), 1.0, 1e6, 100.0)
};
const SEED: ParameterSpec = spec("Seed", Stepping::Integer(1.0), 0.0, u32::MAX as f64, 42.0);
const ITERATION_LIMIT: ParameterSpec = spec("Iteration limit", Stepping::Integer(100.0), 1.0, 1e6, 2_000.0);
const CONVERGENCE_THRESHOLD: ParameterSpec =
    spec("Convergence threshold", Stepping::Factor(10.0), 1e-15, 1e-1, 1e-9);
const ITERATIONS: ParameterSpec = spec("Iterations", Stepping::Integer(50.0), 1.0, 1e6, 500.0);
const THETA: ParameterSpec = spec("Barnes-Hut theta", Stepping::Decimal(0.1), 0.0, 2.0, 0.8);
const SNAPSHOT_INTERVAL: ParameterSpec = spec("Snapshot interval", Stepping::Integer(1.0), 1.0, 1e4, 10.0);

impl ParameterId {
    pub const ALL: [ParameterId; 7] = [
        Self::Bounds,
        Self::Seed,
        Self::IterationLimit,
        Self::ConvergenceThreshold,
        Self::Iterations,
        Self::Theta,
        Self::SnapshotInterval,
    ];

    pub fn spec(self) -> &'static ParameterSpec {
        match self {
            Self::Bounds => &BOUNDS,
            Self::Seed => &SEED,
            Self::IterationLimit => &ITERATION_LIMIT,
            Self::ConvergenceThreshold => &CONVERGENCE_THRESHOLD,
            Self::Iterations => &ITERATIONS,
            Self::Theta => &THETA,
            Self::SnapshotInterval => &SNAPSHOT_INTERVAL,
        }
    }
}

impl ParameterSpec {
    pub fn clamp(&self, value: f64) -> f64 {
        let value = value.clamp(self.min, self.max);
        match self.stepping {
            Stepping::Integer(_) => value.round(),
            Stepping::Factor(_) | Stepping::Decimal(_) => value,
        }
    }

    pub fn step(&self, value: f64, up: bool) -> f64 {
        let stepped = match self.stepping {
            Stepping::Integer(step) | Stepping::Decimal(step) if up => value + step,
            Stepping::Integer(step) | Stepping::Decimal(step) => value - step,
            Stepping::Factor(factor) if up => value * factor,
            Stepping::Factor(factor) => value / factor,
        };
        self.clamp(stepped)
    }
}

/// Half the side of the cube (or square) where `vertices` spread uniformly
/// sit `VERTEX_SPACING` apart: the side is `spacing * n^(1/d)`.
pub fn automatic_bounds(vertices: usize, dimensions: usize) -> f64 {
    let side = VERTEX_SPACING * (vertices.max(1) as f64).powf(1.0 / dimensions as f64);
    (side / 2.0).max(MIN_AUTOMATIC_BOUNDS)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Setting {
    Auto,
    Manual(f64),
}

/// The setting of every parameter, shared by the algorithms that use it.
#[derive(Debug, Clone)]
pub struct ParameterValues(HashMap<ParameterId, Setting>);

impl Default for ParameterValues {
    fn default() -> Self {
        let settings = ParameterId::ALL.into_iter().map(|id| {
            let spec = id.spec();
            let setting = if spec.automatic.is_some() { Setting::Auto } else { Setting::Manual(spec.default) };
            (id, setting)
        });

        Self(settings.collect())
    }
}

impl ParameterValues {
    pub fn get(&self, id: ParameterId) -> Setting {
        self.0[&id]
    }

    /// Clamps manual values. `Auto` is ignored where it does not apply.
    pub fn set(&mut self, id: ParameterId, setting: Setting) {
        let spec = id.spec();
        let setting = match setting {
            Setting::Manual(value) => Setting::Manual(spec.clamp(value)),
            Setting::Auto if spec.automatic.is_none() => return,
            Setting::Auto => Setting::Auto,
        };
        self.0.insert(id, setting);
    }

    /// The value a parameter takes. `Auto` follows the graph, or the default
    /// while there is none.
    pub fn resolve(&self, id: ParameterId, vertices: Option<usize>, dimensions: usize) -> f64 {
        let spec = id.spec();
        match (self.get(id), spec.automatic, vertices) {
            (Setting::Manual(value), _, _) => value,
            (Setting::Auto, Some(derive), Some(vertices)) => spec.clamp(derive(vertices, dimensions)),
            (Setting::Auto, _, _) => spec.default,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_bounds_keep_the_mean_spacing_with_a_floor() {
        // 1000 vertices in 3D: 10 per axis, so a side of 10 spacings.
        let half = automatic_bounds(1_000, 3);
        assert!((half - 5.0 * VERTEX_SPACING).abs() < 1e-9);

        // The same vertices take more room in 2D, where they spread on a plane.
        assert!(automatic_bounds(1_000, 2) > half);
        assert_eq!(automatic_bounds(1, 3), MIN_AUTOMATIC_BOUNDS);
        assert_eq!(automatic_bounds(0, 2), MIN_AUTOMATIC_BOUNDS);
    }

    #[test]
    fn bounds_start_on_auto_and_follow_the_graph() {
        let values = ParameterValues::default();

        assert_eq!(values.get(ParameterId::Bounds), Setting::Auto);
        assert_eq!(values.resolve(ParameterId::Bounds, None, 3), BOUNDS.default);
        assert_eq!(values.resolve(ParameterId::Bounds, Some(1_000), 3), automatic_bounds(1_000, 3));
    }

    #[test]
    fn manual_values_are_clamped_and_auto_needs_a_derivation() {
        let mut values = ParameterValues::default();
        values.set(ParameterId::Bounds, Setting::Manual(1e9));
        values.set(ParameterId::Seed, Setting::Auto);

        assert_eq!(values.resolve(ParameterId::Bounds, Some(1_000), 3), BOUNDS.max);
        assert_eq!(values.get(ParameterId::Seed), Setting::Manual(SEED.default));
    }

    #[test]
    fn steps_follow_the_kind_of_parameter() {
        assert_eq!(SEED.step(42.0, true), 43.0);
        assert_eq!(SEED.step(0.0, false), 0.0);
        assert!((CONVERGENCE_THRESHOLD.step(1e-9, true) - 1e-8).abs() < 1e-20);
        assert_eq!(ITERATION_LIMIT.clamp(150.4), 150.0);
    }
}
