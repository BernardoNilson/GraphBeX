use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;
use crossbeam_channel::Receiver;

use crate::modules::application::processing::GraphUpdate;
use crate::modules::common::graph::Graph;
use crate::modules::data::vertex_attributes::VertexAttributes;

/// Drains the graph channel into resources, once per frame, so the rendering
/// and the interface read the same state without competing for messages.
pub struct GraphStatePlugin(pub Receiver<GraphUpdate>);

#[derive(Resource)]
struct GraphChannel(Receiver<GraphUpdate>);

#[derive(Resource, Default)]
pub struct LoadedGraph {
    pub graph: Option<Arc<dyn Graph>>,
    pub attributes: VertexAttributes,
    pub path: Option<PathBuf>,
    /// How long reading the file and building the CSR took.
    pub load_time: Option<Duration>,
}

impl LoadedGraph {
    pub fn file_name(&self) -> Option<String> {
        Some(self.path.as_ref()?.file_name()?.to_string_lossy().to_string())
    }

    pub fn node_count(&self) -> Option<usize> {
        self.graph.as_ref().map(|graph| graph.node_count())
    }
}

/// The latest snapshot of the positions, indexed by vertex.
#[derive(Resource, Default)]
pub struct LayoutPositions {
    pub positions: Vec<Vec3>,
    /// The run the snapshot belongs to.
    pub run: u64,
}

/// Shown in the status bar by its name.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutStatus {
    #[default]
    Idle,
    Running,
    Converged,
    Interrupted,
}

#[derive(Resource, Debug, Clone)]
pub struct LayoutRunInfo {
    pub run: u64,
    pub algorithm: Option<&'static str>,
    /// Decides the camera mode (FR3).
    pub dimensions: usize,
    pub iterations: usize,
    pub elapsed: Option<Duration>,
}

impl Default for LayoutRunInfo {
    fn default() -> Self {
        Self { run: 0, algorithm: None, dimensions: 3, iterations: 0, elapsed: None }
    }
}

/// The attribute the vertices are colored by (View > Color By), if any.
#[derive(Resource, Default)]
pub struct VertexColoring(pub Option<usize>);

/// The selected vertex (FR6), if any.
#[derive(Resource, Default, PartialEq)]
pub struct VertexSelection(pub Option<u32>);

#[derive(Resource, Default)]
pub struct LastError(pub Option<String>);

impl Plugin for GraphStatePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GraphChannel(self.0.clone()))
            .init_resource::<LoadedGraph>()
            .init_resource::<LayoutPositions>()
            .init_resource::<LayoutStatus>()
            .init_resource::<LayoutRunInfo>()
            .init_resource::<LastError>()
            .init_resource::<VertexColoring>()
            .init_resource::<VertexSelection>()
            // Before `Update`, so every system sees the latest state.
            .add_systems(PreUpdate, receive_graph_updates);
    }
}

/// Reads the channel without blocking, keeping only the latest snapshot.
fn receive_graph_updates(
    channel: Res<GraphChannel>,
    mut loaded: ResMut<LoadedGraph>,
    mut positions: ResMut<LayoutPositions>,
    mut status: ResMut<LayoutStatus>,
    mut info: ResMut<LayoutRunInfo>,
    mut last_error: ResMut<LastError>,
    mut coloring: ResMut<VertexColoring>,
    mut selection: ResMut<VertexSelection>,
) {
    for update in channel.0.try_iter() {
        match update {
            GraphUpdate::Loaded { graph, attributes, path, load_time } => {
                *loaded = LoadedGraph { graph: Some(graph), attributes, path: Some(path), load_time: Some(load_time) };
                positions.positions.clear();
                *status = LayoutStatus::Idle;
                coloring.0 = None;
                selection.0 = None;
                last_error.0 = None;
            }
            GraphUpdate::Positions { positions: snapshot, iterations } => {
                positions.positions = snapshot;
                positions.run = info.run;
                info.iterations = iterations;
            }
            GraphUpdate::LayoutStarted { algorithm, dimensions } => {
                log::info!("Layout started: {algorithm} in {dimensions}D.");
                *info = LayoutRunInfo { run: info.run + 1, algorithm: Some(algorithm), dimensions, ..default() };
                *status = LayoutStatus::Running;
                last_error.0 = None;
            }
            GraphUpdate::LayoutFinished { interrupted, iterations, elapsed } => {
                info.iterations = iterations;
                info.elapsed = Some(elapsed);
                *status = if interrupted { LayoutStatus::Interrupted } else { LayoutStatus::Converged };
            }
            GraphUpdate::Failed(message) => {
                log::error!("Processing engine: {message}.");
                last_error.0 = Some(message);
            }
        }
    }
}
