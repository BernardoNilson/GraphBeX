//! Vertex selection (FR6): a click selects the vertex drawn under the pointer,
//! and a click on empty space clears the selection.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::camera::OrbitCamera;
use super::menu::OpenMenu;
use crate::modules::presentation::graph_state::{LayoutPositions, VertexSelection};
use crate::modules::presentation::rendering::VERTEX_RADIUS;

/// How far the pointer may move, in pixels, for a press to be a click and
/// not a drag of the camera.
const CLICK_TOLERANCE: f32 = 4.0;

/// The smallest radius, in pixels, a vertex can be clicked within, so far
/// vertices stay clickable.
const MIN_PICK_RADIUS: f32 = 6.0;

pub fn select_vertex(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    interface: Query<&Interaction>,
    open_menu: Res<OpenMenu>,
    positions: Res<LayoutPositions>,
    mut selection: ResMut<VertexSelection>,
    mut pressed_at: Local<Option<Vec2>>,
) {
    let cursor = windows.single().ok().and_then(Window::cursor_position);

    if mouse.just_pressed(MouseButton::Left) {
        let over_interface = open_menu.0.is_some() || interface.iter().any(|i| *i != Interaction::None);
        *pressed_at = cursor.filter(|_| !over_interface);
    }

    if !mouse.just_released(MouseButton::Left) {
        return;
    }
    let (Some(start), Some(end), Ok((camera, transform))) = (pressed_at.take(), cursor, cameras.single()) else {
        return;
    };

    if start.distance(end) <= CLICK_TOLERANCE {
        let picked = pick(camera, transform, &positions.positions, end);
        selection.set_if_neq(VertexSelection(picked));
    }
}

/// Among the vertices whose drawn disk holds the cursor, the closest to the
/// camera. On a tie, as in 2D, the closest to the cursor.
fn pick(camera: &Camera, transform: &GlobalTransform, positions: &[Vec3], cursor: Vec2) -> Option<u32> {
    let rim = transform.right() * VERTEX_RADIUS;

    positions
        .iter()
        .enumerate()
        .filter_map(|(index, &position)| {
            let centre = camera.world_to_viewport_with_depth(transform, position).ok()?;
            let edge = camera.world_to_viewport(transform, position + rim).ok()?;
            let distance = centre.truncate().distance(cursor);
            let radius = centre.truncate().distance(edge).max(MIN_PICK_RADIUS);

            (distance <= radius).then_some((centre.z, distance, index as u32))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)))
        .map(|(.., index)| index)
}
