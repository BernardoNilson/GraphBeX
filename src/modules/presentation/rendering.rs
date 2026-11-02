use std::sync::Arc;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::prelude::*;
use bevy_polyline::prelude::*;

use super::instancing::{Instance, InstancingPlugin, VertexInstances};
use super::interaction::theme::{Mode, Tone};
use crate::modules::data::vertex_attributes::NO_CATEGORY;
use crate::modules::presentation::graph_state::{LayoutPositions, LoadedGraph, VertexColoring, VertexSelection};

pub const VERTEX_RADIUS: f32 = 2.5;

/// Above this many vertices the sphere has 20 triangles instead of 320, so the
/// GPU keeps up with large graphs.
const DETAILED_VERTEX_LIMIT: usize = 100_000;

/// The colors of the Color By categories, repeated when there are more.
const PALETTE: [Color; 12] = [
    Color::srgb(0.31, 0.47, 0.65),
    Color::srgb(0.95, 0.56, 0.17),
    Color::srgb(0.88, 0.34, 0.35),
    Color::srgb(0.46, 0.72, 0.70),
    Color::srgb(0.35, 0.63, 0.31),
    Color::srgb(0.93, 0.79, 0.28),
    Color::srgb(0.69, 0.48, 0.63),
    Color::srgb(1.00, 0.62, 0.66),
    Color::srgb(0.61, 0.46, 0.37),
    Color::srgb(0.73, 0.69, 0.67),
    Color::srgb(0.55, 0.80, 0.98),
    Color::srgb(0.70, 0.85, 0.40),
];

/// How many edges share one polyline, and so one draw call.
const EDGES_PER_BATCH: usize = 1 << 16;

/// The GPU drops the segments touching a NaN point, so each edge is written
/// as `a, b, NaN` and the edges of a batch stay apart.
const BREAK: Vec3 = Vec3::NAN;

pub struct Rendering;

/// The entity that draws every vertex.
#[derive(Component)]
struct Vertices;

/// The vertex pairs one polyline draws.
#[derive(Component)]
struct EdgeBatch(Vec<(u32, u32)>);

/// The edges of the selected vertex, drawn thicker on top of the others.
#[derive(Component)]
struct SelectionEdges;

#[derive(Resource)]
struct SceneAssets {
    detailed: Handle<Mesh>,
    light: Handle<Mesh>,
    edges: Handle<PolylineMaterial>,
    selection_edges: Handle<PolylineMaterial>,
}

impl Plugin for Rendering {
    fn build(&self, app: &mut App) {
        let moved = resource_changed::<LayoutPositions>;
        let restyled = resource_changed::<Mode>;
        let recolor = moved.or(resource_changed::<VertexColoring>).or(resource_changed::<VertexSelection>).or(restyled);
        let reselect = moved.or(resource_changed::<VertexSelection>);

        app.add_plugins((PolylinePlugin, InstancingPlugin))
            .add_systems(Startup, setup_scene)
            .add_systems(
                Update,
                (
                    spawn_graph.run_if(resource_changed::<LoadedGraph>),
                    // The entities spawned above only exist once the commands apply.
                    ApplyDeferred,
                    paint_scene.run_if(restyled),
                    update_vertices.run_if(recolor),
                    update_edges.run_if(moved),
                    update_selection_edges.run_if(reselect),
                )
                    .chain(),
            );
    }
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut polylines: ResMut<Assets<Polyline>>,
    mut materials: ResMut<Assets<PolylineMaterial>>,
) {
    let mut sphere = |subdivisions| {
        let mesh = Sphere::new(VERTEX_RADIUS).mesh().ico(subdivisions);
        meshes.add(mesh.expect("a low subdivision count is valid"))
    };
    // The colors come from `paint_scene`.
    let mut line = |width, depth_bias| materials.add(PolylineMaterial { width, depth_bias, ..default() });
    let selection_edges = line(3.0, -0.0005);

    commands.spawn((
        SelectionEdges,
        PolylineBundle {
            polyline: PolylineHandle(polylines.add(Polyline { vertices: Vec::new() })),
            material: PolylineMaterialHandle(selection_edges.clone()),
            ..default()
        },
    ));
    commands.insert_resource(SceneAssets { detailed: sphere(2), light: sphere(0), edges: line(1.0, 0.0), selection_edges });
}

/// The background and the edges in the colors of the mode.
fn paint_scene(
    mode: Res<Mode>,
    assets: Res<SceneAssets>,
    mut background: ResMut<ClearColor>,
    mut materials: ResMut<Assets<PolylineMaterial>>,
) {
    background.0 = mode.color(Tone::Background);
    for (material, tone) in [(&assets.edges, Tone::Edge), (&assets.selection_edges, Tone::Accent)] {
        if let Some(material) = materials.get_mut(material) {
            material.color = mode.color(tone).into();
        }
    }
}

/// Rebuilds the drawn graph whenever another one is loaded. The positions
/// arrive later, in `update_vertices` and `update_edges`.
fn spawn_graph(
    mut commands: Commands,
    loaded: Res<LoadedGraph>,
    assets: Res<SceneAssets>,
    mut polylines: ResMut<Assets<Polyline>>,
    drawn: Query<Entity, Or<(With<Vertices>, With<EdgeBatch>)>>,
) {
    drawn.iter().for_each(|entity| commands.entity(entity).despawn());

    let Some(graph) = &loaded.graph else {
        return;
    };
    let count = graph.node_count();
    let mesh = if count > DETAILED_VERTEX_LIMIT { &assets.light } else { &assets.detailed };
    commands.spawn((Vertices, Mesh3d(mesh.clone()), VertexInstances::default(), NoFrustumCulling));

    // Each undirected edge is drawn once. A self-loop would be a point.
    let edges: Vec<(u32, u32)> = (0..count as u32)
        .flat_map(|source| graph.neighbors(source).iter().filter(move |&&t| source < t).map(move |&t| (source, t)))
        .collect();

    for batch in edges.chunks(EDGES_PER_BATCH) {
        commands.spawn((
            EdgeBatch(batch.to_vec()),
            PolylineBundle {
                polyline: PolylineHandle(polylines.add(Polyline { vertices: Vec::new() })),
                material: PolylineMaterialHandle(assets.edges.clone()),
                ..default()
            },
        ));
    }

    let batches = edges.len().div_ceil(EDGES_PER_BATCH);
    log::info!("Rendering spawned {count} vertices and {} edges in {batches} batches.", edges.len());
}

/// Places and colors every vertex instance. The selected vertex and its
/// neighbors take the highlights (FR6), the others their Color By category.
fn update_vertices(
    positions: Res<LayoutPositions>,
    coloring: Res<VertexColoring>,
    selection: Res<VertexSelection>,
    loaded: Res<LoadedGraph>,
    mode: Res<Mode>,
    mut vertices: Query<&mut VertexInstances>,
) {
    let Ok(mut instances) = vertices.single_mut() else {
        return;
    };
    let categories = coloring.0.and_then(|index| loaded.attributes.0.get(index)).map(|a| &a.categories);
    let (selected, neighbors) = neighborhood(&selection, &loaded).unwrap_or((u32::MAX, &[]));
    let palette = PALETTE.map(linear);
    let [plain, highlight, center] =
        [Tone::Vertex, Tone::NeighborVertex, Tone::SelectedVertex].map(|tone| linear(mode.color(tone)));

    let instance = |(index, &position): (usize, &Vec3)| {
        let vertex = index as u32;
        let category = categories.and_then(|categories| categories.get(index)).filter(|&&c| c != NO_CATEGORY);
        let color = match category {
            _ if vertex == selected => center,
            _ if neighbors.binary_search(&vertex).is_ok() => highlight,
            Some(&category) => palette[category as usize % PALETTE.len()],
            None => plain,
        };
        Instance { position, color }
    };

    instances.0 = Arc::new(positions.positions.iter().enumerate().map(instance).collect());
}

fn update_edges(
    positions: Res<LayoutPositions>,
    batches: Query<(&EdgeBatch, &PolylineHandle)>,
    mut polylines: ResMut<Assets<Polyline>>,
) {
    for (batch, handle) in &batches {
        if let Some(polyline) = polylines.get_mut(&handle.0) {
            polyline.vertices = batch_vertices(&batch.0, &positions.positions);
        }
    }
}

fn update_selection_edges(
    positions: Res<LayoutPositions>,
    selection: Res<VertexSelection>,
    loaded: Res<LoadedGraph>,
    edges: Query<&PolylineHandle, With<SelectionEdges>>,
    mut polylines: ResMut<Assets<Polyline>>,
) {
    let pairs: Vec<(u32, u32)> = match neighborhood(&selection, &loaded) {
        Some((vertex, neighbors)) => neighbors.iter().map(|&neighbor| (vertex, neighbor)).collect(),
        None => Vec::new(),
    };

    for handle in &edges {
        if let Some(polyline) = polylines.get_mut(&handle.0) {
            polyline.vertices = batch_vertices(&pairs, &positions.positions);
        }
    }
}

/// The selected vertex and its neighbors, if a vertex is selected.
fn neighborhood<'a>(selection: &VertexSelection, loaded: &'a LoadedGraph) -> Option<(u32, &'a [u32])> {
    let vertex = selection.0?;
    Some((vertex, loaded.graph.as_ref()?.neighbors(vertex)))
}

fn linear(color: Color) -> [f32; 4] {
    color.to_linear().to_f32_array()
}

fn batch_vertices(edges: &[(u32, u32)], positions: &[Vec3]) -> Vec<Vec3> {
    edges
        .iter()
        .flat_map(|&(source, target)| [position_of(positions, source), position_of(positions, target), BREAK])
        .collect()
}

/// Where a vertex is, or the origin while no snapshot has arrived.
fn position_of(positions: &[Vec3], vertex: u32) -> Vec3 {
    positions.get(vertex as usize).copied().unwrap_or(Vec3::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_edge_is_followed_by_a_break() {
        let vertices = batch_vertices(&[(0, 1), (1, 2)], &[Vec3::X, Vec3::Y, Vec3::Z]);

        assert_eq!(vertices.len(), 6);
        assert_eq!(&vertices[..2], &[Vec3::X, Vec3::Y]);
        assert!(vertices[2].is_nan());
        assert_eq!(&vertices[3..5], &[Vec3::Y, Vec3::Z]);
        assert!(vertices[5].is_nan());
    }
}
