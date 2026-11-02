use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use bevy::math::Vec3;
use crossbeam_channel::{Receiver, Sender, unbounded};

use super::layout_parameters::{ParameterId, ParameterValues, Setting};
use crate::modules::common::fruchterman_reingold_layout::FruchtermanReingold;
use crate::modules::common::graph::Graph;
use crate::modules::common::layout_algorithm::LayoutAlgorithm;
use crate::modules::common::random_layout::RandomLayout;
use crate::modules::common::spectral_layout::SpectralLayout;
use crate::modules::data::ingestion;
use crate::modules::data::vertex_attributes::VertexAttributes;

/// The dimensions a layout can be computed in (FR3).
pub const AVAILABLE_DIMENSIONS: [usize; 2] = [2, 3];

/// Every loaded graph starts from the random layout, the initial state the
/// TCC describes for the other algorithms.
const INITIAL_LAYOUT: LayoutKind = LayoutKind::Random;

/// Commands from the interface to the processing engine.
#[derive(Debug, Clone)]
pub enum LayoutCommand {
    LoadGraph(PathBuf),
    SelectAlgorithm(LayoutKind),
    SetDimensions(usize),
    SetParameter(ParameterId, Setting),
    Start,
    Stop,
}

/// Updates from the processing engine to the presentation.
pub enum GraphUpdate {
    /// The graph travels along: drawing the edges needs the topology.
    Loaded {
        graph: Arc<dyn Graph>,
        attributes: VertexAttributes,
        path: PathBuf,
        load_time: Duration,
    },
    LayoutStarted {
        algorithm: &'static str,
        dimensions: usize,
    },
    /// A snapshot of the positions, indexed by vertex.
    Positions {
        positions: Vec<Vec3>,
        iterations: usize,
    },
    LayoutFinished {
        interrupted: bool,
        iterations: usize,
        elapsed: Duration,
    },
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutKind {
    Random,
    Spectral,
    ForceDirected,
}

impl LayoutKind {
    pub const ALL: [LayoutKind; 3] = [Self::Random, Self::Spectral, Self::ForceDirected];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Random => "Random",
            Self::Spectral => "Spectral",
            Self::ForceDirected => "Force-Directed",
        }
    }

    /// The parameters of the algorithm, in display order. The spectral limit
    /// and threshold are those of its eigensolver.
    pub const fn parameters(self) -> &'static [ParameterId] {
        use ParameterId::*;

        match self {
            Self::Random => &[Bounds, Seed],
            Self::Spectral => &[Bounds, IterationLimit, ConvergenceThreshold],
            Self::ForceDirected => &[Bounds, Seed, Iterations, Theta, SnapshotInterval],
        }
    }

    fn build(self, dimensions: usize, parameters: &ParameterValues, vertices: usize) -> Box<dyn LayoutAlgorithm> {
        let value = |id| parameters.resolve(id, Some(vertices), dimensions);
        let bounds = value(ParameterId::Bounds) as f32;
        let seed = value(ParameterId::Seed) as u64;

        match self {
            Self::Random => Box::new(RandomLayout::new(dimensions, bounds, seed)),
            Self::Spectral => Box::new(SpectralLayout::new(dimensions, bounds).with_solver(
                value(ParameterId::IterationLimit) as usize,
                value(ParameterId::ConvergenceThreshold),
            )),
            Self::ForceDirected => Box::new(
                FruchtermanReingold::new(dimensions, bounds, seed, value(ParameterId::Iterations) as usize)
                    .with_barnes_hut(value(ParameterId::Theta) as f32),
            ),
        }
    }
}

/// Starts the processing engine on its own thread, and returns the channel
/// ends the interface uses: commands in, updates out.
pub fn start() -> (Sender<LayoutCommand>, Receiver<GraphUpdate>) {
    let (commands, command_receiver) = unbounded();
    let (update_sender, updates) = unbounded();

    let engine = Engine {
        updates: update_sender,
        graph: None,
        kind: LayoutKind::Random,
        dimensions: 3,
        parameters: ParameterValues::default(),
        run: None,
    };
    thread::spawn(move || engine.run(command_receiver));

    (commands, updates)
}

struct Engine {
    updates: Sender<GraphUpdate>,
    graph: Option<Arc<dyn Graph>>,
    kind: LayoutKind,
    dimensions: usize,
    parameters: ParameterValues,
    run: Option<LayoutRun>,
}

impl Engine {
    /// Handles commands until every sender is dropped, when the window closes.
    /// Waiting for the next command takes no CPU.
    fn run(mut self, commands: Receiver<LayoutCommand>) {
        for command in commands {
            match command {
                LayoutCommand::LoadGraph(path) => self.load(path),
                LayoutCommand::SelectAlgorithm(kind) => {
                    self.interrupt();
                    self.kind = kind;
                    log::info!("Layout algorithm selected: {}.", kind.label());
                }
                LayoutCommand::SetDimensions(dimensions) => {
                    self.interrupt();
                    self.dimensions = dimensions;
                    log::info!("Layout dimensions selected: {dimensions}D.");
                }
                LayoutCommand::SetParameter(id, setting) => {
                    self.parameters.set(id, setting);
                    log::info!("Layout parameter {id:?} set to {setting:?}.");
                }
                LayoutCommand::Start => self.start_run(self.kind),
                LayoutCommand::Stop => self.interrupt(),
            }
        }

        self.interrupt();
        log::info!("Processing engine stopped.");
    }

    fn load(&mut self, path: PathBuf) {
        self.interrupt();
        let started = Instant::now();

        match ingestion::load(&path) {
            Ok((graph, attributes)) => {
                let load_time = started.elapsed();
                log::info!(
                    "Graph loaded from '{}' with {} vertices and {} edges in {load_time:?}.",
                    path.display(),
                    graph.node_count(),
                    graph.edge_count()
                );

                self.graph = Some(Arc::clone(&graph));
                self.send(GraphUpdate::Loaded { graph, attributes, path, load_time });
                self.start_run(INITIAL_LAYOUT);
            }
            Err(error) => {
                self.send(GraphUpdate::Failed(format!("could not load '{}': {error}", path.display())))
            }
        }
    }

    fn start_run(&mut self, kind: LayoutKind) {
        self.interrupt();

        let Some(graph) = self.graph.clone() else {
            return self.send(GraphUpdate::Failed("no graph loaded yet".to_string()));
        };

        let layout = kind.build(self.dimensions, &self.parameters, graph.node_count());
        let interval = self.parameters.resolve(ParameterId::SnapshotInterval, None, self.dimensions);

        self.send(GraphUpdate::LayoutStarted { algorithm: kind.label(), dimensions: self.dimensions });
        self.run = Some(LayoutRun::start(graph, layout, interval as usize, self.updates.clone()));
    }

    fn interrupt(&mut self) {
        if let Some(run) = self.run.take() {
            run.interrupt();
        }
    }

    /// A failed send means the window closed, so there is no one to tell.
    fn send(&self, update: GraphUpdate) {
        let _ = self.updates.send(update);
    }
}

/// A layout run on a thread of its own, so it never blocks the engine or the
/// rendering. The thread ends on convergence or interruption.
struct LayoutRun {
    stop: Arc<AtomicBool>,
    thread: thread::JoinHandle<()>,
}

impl LayoutRun {
    fn start(
        graph: Arc<dyn Graph>,
        mut layout: Box<dyn LayoutAlgorithm>,
        snapshot_interval: usize,
        updates: Sender<GraphUpdate>,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        log::info!("Starting the {} algorithm.", layout.name());

        let thread = thread::spawn(move || {
            let started = Instant::now();
            let mut positions = vec![Vec3::ZERO; graph.node_count()];
            let mut iterations = 0;

            let interrupted = loop {
                let more = layout.step(graph.as_ref(), &mut positions);
                iterations += 1;

                if !more {
                    break false;
                }
                if stopped.load(Ordering::Relaxed) {
                    break true;
                }
                if iterations % snapshot_interval.max(1) == 0 {
                    let snapshot = GraphUpdate::Positions { positions: positions.clone(), iterations };
                    if updates.send(snapshot).is_err() {
                        return;
                    }
                }
            };

            let elapsed = started.elapsed();
            let how = if interrupted { "interrupted" } else { "converged" };
            log::info!("Layout run {how} after {iterations} iterations in {elapsed:?}.");

            // Both endings send a final snapshot.
            let _ = updates.send(GraphUpdate::Positions { positions, iterations });
            let _ = updates.send(GraphUpdate::LayoutFinished { interrupted, iterations, elapsed });
        });

        Self { stop, thread }
    }

    fn interrupt(self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.thread.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temporary edge list of a three-vertex path, removed when dropped.
    struct TempGraph(PathBuf);

    impl TempGraph {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(name);
            std::fs::write(&path, "0 1\n1 2\n").expect("failed to write the temp file");
            Self(path)
        }

        fn load(&self) -> LayoutCommand {
            LayoutCommand::LoadGraph(self.0.clone())
        }
    }

    impl Drop for TempGraph {
        fn drop(&mut self) {
            std::fs::remove_file(&self.0).ok();
        }
    }

    /// Starts an engine and sends it the commands. The sender is returned,
    /// since dropping it stops the engine.
    fn engine(commands: impl IntoIterator<Item = LayoutCommand>) -> (Sender<LayoutCommand>, Receiver<GraphUpdate>) {
        let (sender, updates) = start();
        for command in commands {
            sender.send(command).unwrap();
        }
        (sender, updates)
    }

    struct Run {
        algorithm: &'static str,
        dimensions: usize,
        positions: Vec<Vec3>,
        iterations: usize,
        snapshots: usize,
    }

    /// Reads updates until a run finishes, failing on any error.
    fn wait_for_run(updates: &Receiver<GraphUpdate>) -> Run {
        let (mut started, mut positions, mut snapshots) = (None, Vec::new(), 0);

        for update in updates {
            match update {
                GraphUpdate::LayoutStarted { algorithm, dimensions } => started = Some((algorithm, dimensions)),
                GraphUpdate::Positions { positions: snapshot, .. } => {
                    positions = snapshot;
                    snapshots += 1;
                }
                GraphUpdate::LayoutFinished { iterations, .. } => {
                    let (algorithm, dimensions) = started.expect("the run should report its start");
                    return Run { algorithm, dimensions, positions, iterations, snapshots };
                }
                GraphUpdate::Failed(message) => panic!("the engine failed: {message}"),
                GraphUpdate::Loaded { .. } => (),
            }
        }

        panic!("the engine stopped before the run finished");
    }

    #[test]
    fn a_loaded_graph_is_laid_out_randomly_whatever_is_selected() {
        let file = TempGraph::new("processing_load_test.el");
        let (_commands, updates) = engine([LayoutCommand::SelectAlgorithm(LayoutKind::Spectral), file.load()]);

        match updates.recv().unwrap() {
            GraphUpdate::Loaded { graph, .. } => assert_eq!(graph.node_count(), 3),
            _ => panic!("expected the loaded graph"),
        }

        let run = wait_for_run(&updates);
        assert_eq!((run.algorithm, run.positions.len(), run.iterations), ("Random", 3, 1));
    }

    #[test]
    fn start_runs_the_selected_algorithm() {
        let file = TempGraph::new("processing_spectral_test.el");
        let (commands, updates) = engine([file.load(), LayoutCommand::SelectAlgorithm(LayoutKind::Spectral)]);
        wait_for_run(&updates);

        commands.send(LayoutCommand::Start).unwrap();

        let run = wait_for_run(&updates);
        assert_eq!(run.algorithm, "Spectral");
        assert!(run.positions.iter().any(|position| *position != Vec3::ZERO));
    }

    #[test]
    fn the_force_directed_layout_sends_periodic_snapshots() {
        let file = TempGraph::new("processing_force_directed_test.el");
        let set = |id, value| LayoutCommand::SetParameter(id, Setting::Manual(value));
        let (commands, updates) = engine([
            file.load(),
            set(ParameterId::Iterations, 20.0),
            set(ParameterId::SnapshotInterval, 5.0),
            LayoutCommand::SelectAlgorithm(LayoutKind::ForceDirected),
        ]);
        wait_for_run(&updates);

        commands.send(LayoutCommand::Start).unwrap();

        let run = wait_for_run(&updates);
        assert_eq!(run.iterations, 20);
        // Every 5 iterations (5, 10, 15) plus the final snapshot.
        assert_eq!(run.snapshots, 4);
    }

    #[test]
    fn honours_the_dimensions_and_manual_bounds() {
        let file = TempGraph::new("processing_settings_test.el");
        // Set before loading, so the run that follows the load uses them.
        let (_commands, updates) = engine([
            LayoutCommand::SetDimensions(2),
            LayoutCommand::SetParameter(ParameterId::Bounds, Setting::Manual(2.0)),
            file.load(),
        ]);

        let run = wait_for_run(&updates);
        assert_eq!(run.dimensions, 2);
        assert!(run.positions.iter().all(|p| p.z == 0.0 && p.abs().max_element() <= 2.0));
    }

    #[test]
    fn reports_a_start_without_a_graph_and_a_failed_load() {
        for command in [LayoutCommand::Start, LayoutCommand::LoadGraph(PathBuf::from("missing_file.el"))] {
            let (_commands, updates) = engine([command]);
            assert!(matches!(updates.recv().unwrap(), GraphUpdate::Failed(_)));
        }
    }
}
