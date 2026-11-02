//! Camera navigation (FR5) in the modes of FR3. In 2D, a fixed front view with
//! pan and zoom. In 3D, an orbit around the graph centroid. The mode follows
//! the dimensions of the computed layout.

use std::f32::consts::FRAC_PI_4;

use bevy::camera::ScalingMode;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::render::view::NoIndirectDrawing;
use bevy::window::PrimaryWindow;

use crate::modules::presentation::graph_state::{LayoutPositions, LayoutRunInfo, LoadedGraph};

/// The vertical field of view of the 3D camera, Bevy's default.
const FIELD_OF_VIEW: f32 = FRAC_PI_4;

/// The room left around the graph when it is framed.
const FRAME_MARGIN: f32 = 1.15;

/// The distance limits, as multiples of the graph radius.
const MIN_DISTANCE_RATIO: f32 = 0.02;
const MAX_DISTANCE_RATIO: f32 = 20.0;

const MIN_RADIUS: f32 = 10.0;
const ROTATION_SPEED: f32 = 0.006;
const PAN_SPEED: f32 = 0.0022;
const ZOOM_SPEED: f32 = 0.12;

/// Keeps the orbit from flipping over the poles.
const MAX_PITCH: f32 = 1.54;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    /// Orthographic, from the front.
    TwoD,
    /// Perspective, orbiting the focus.
    ThreeD,
}

/// Looks at `focus` from `distance` away. In 2D the orientation is fixed and
/// `distance` sets the zoom, matching what the 3D view would show.
#[derive(Component)]
pub struct OrbitCamera {
    pub mode: ViewMode,
    pub focus: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    /// The radius of the framed graph, which sets the limits and far plane.
    pub radius: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        let radius = 100.0;
        Self { mode: ViewMode::ThreeD, focus: Vec3::ZERO, distance: framing_distance(radius), yaw: 0.0, pitch: 0.0, radius }
    }
}

impl OrbitCamera {
    fn min_distance(&self) -> f32 {
        self.radius * MIN_DISTANCE_RATIO
    }

    fn max_distance(&self) -> f32 {
        self.radius * MAX_DISTANCE_RATIO
    }

    /// The height of the world visible at the focus.
    fn visible_height(&self) -> f32 {
        2.0 * self.distance * (FIELD_OF_VIEW / 2.0).tan()
    }

    fn frame(&mut self, centre: Vec3, radius: f32, reset_orientation: bool) {
        self.focus = centre;
        self.radius = radius.max(MIN_RADIUS);
        self.distance = framing_distance(self.radius);

        if reset_orientation || self.mode == ViewMode::TwoD {
            (self.yaw, self.pitch) = (0.0, 0.0);
        }
    }
}

/// How far the camera must stand to see a sphere of the given radius whole.
fn framing_distance(radius: f32) -> f32 {
    radius * FRAME_MARGIN / (FIELD_OF_VIEW / 2.0).sin()
}

#[derive(Resource, Default)]
pub struct ViewRequests {
    /// Frame the whole graph again from the default orientation.
    pub reset: bool,
}

pub fn spawn_camera(mut commands: Commands) {
    let orbit = OrbitCamera::default();
    // The vertex instances are drawn with direct, not indirect, draw calls.
    commands.spawn((Camera3d::default(), NoIndirectDrawing, projection(&orbit), orbit_transform(&orbit), orbit));
}

/// Frames the graph when it is new, when another algorithm or dimension is
/// laid out, and on request. Running the same layout again with other
/// parameters keeps the camera where the user left it.
pub fn frame_graph(
    loaded: Res<LoadedGraph>,
    positions: Res<LayoutPositions>,
    info: Res<LayoutRunInfo>,
    mut requests: ResMut<ViewRequests>,
    mut seen_run: Local<u64>,
    mut framed: Local<Option<(Option<&'static str>, usize)>>,
    mut cameras: Query<&mut OrbitCamera>,
) {
    if loaded.is_changed() {
        *framed = None;
    }
    let reset = std::mem::take(&mut requests.reset);
    let layout = (info.algorithm, info.dimensions);

    let new_run = positions.run != *seen_run && !positions.positions.is_empty();
    if new_run {
        *seen_run = positions.run;
    }
    if !(new_run && *framed != Some(layout)) && !reset {
        return;
    }
    let Some((centre, radius)) = extent(&positions.positions) else {
        return;
    };
    let mode = if info.dimensions == 2 { ViewMode::TwoD } else { ViewMode::ThreeD };

    for mut orbit in &mut cameras {
        let reset_orientation = reset || framed.is_none() || orbit.mode != mode;
        orbit.mode = mode;
        orbit.frame(centre, radius, reset_orientation);
    }
    *framed = Some(layout);
}

/// The centroid, and the distance from it to the furthest position.
fn extent(positions: &[Vec3]) -> Option<(Vec3, f32)> {
    if positions.is_empty() {
        return None;
    }
    let centre = positions.iter().copied().sum::<Vec3>() / positions.len() as f32;
    let radius = positions.iter().map(|position| position.distance(centre)).fold(0.0, f32::max);
    Some((centre, radius))
}

/// In 3D, left drag orbits and right drag pans. In 2D both pan. The wheel
/// zooms. Input over the interface is ignored.
pub fn control_camera(
    motion: Res<AccumulatedMouseMotion>,
    wheel: Res<AccumulatedMouseScroll>,
    buttons: Res<ButtonInput<MouseButton>>,
    interface: Query<&Interaction>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cameras: Query<(&mut OrbitCamera, &Transform)>,
) {
    let drag = motion.delta;
    // Trackpad pixels are scaled to roughly one wheel notch.
    let scroll = match wheel.unit {
        MouseScrollUnit::Line => wheel.delta.y,
        MouseScrollUnit::Pixel => wheel.delta.y / 20.0,
    };
    let over_interface = interface.iter().any(|interaction| *interaction != Interaction::None);

    if over_interface || (drag == Vec2::ZERO && scroll == 0.0) {
        return;
    }

    let window_height = windows.single().map_or(720.0, |window| window.height());
    let (left, right) = (buttons.pressed(MouseButton::Left), buttons.pressed(MouseButton::Right));

    for (mut orbit, transform) in &mut cameras {
        match orbit.mode {
            ViewMode::ThreeD if left => {
                orbit.yaw -= drag.x * ROTATION_SPEED;
                orbit.pitch = (orbit.pitch - drag.y * ROTATION_SPEED).clamp(-MAX_PITCH, MAX_PITCH);
            }
            ViewMode::ThreeD if right => {
                let speed = orbit.distance * PAN_SPEED;
                orbit.focus += (-drag.x * transform.right().as_vec3() + drag.y * transform.up().as_vec3()) * speed;
            }
            // In 2D the graph follows the pointer.
            ViewMode::TwoD if left || right => {
                let speed = orbit.visible_height() / window_height;
                orbit.focus += Vec3::new(-drag.x, drag.y, 0.0) * speed;
            }
            _ => (),
        }

        if scroll != 0.0 {
            let distance = orbit.distance * (1.0 - scroll * ZOOM_SPEED);
            orbit.distance = distance.clamp(orbit.min_distance(), orbit.max_distance());
        }
    }
}

pub fn apply_camera(mut cameras: Query<(&OrbitCamera, &mut Transform, &mut Projection), Changed<OrbitCamera>>) {
    for (orbit, mut transform, mut current) in &mut cameras {
        *transform = orbit_transform(orbit);
        *current = projection(orbit);
    }
}

fn orbit_transform(orbit: &OrbitCamera) -> Transform {
    let rotation = match orbit.mode {
        ViewMode::ThreeD => Quat::from_rotation_y(orbit.yaw) * Quat::from_rotation_x(orbit.pitch),
        ViewMode::TwoD => Quat::IDENTITY,
    };
    Transform::from_translation(orbit.focus + rotation * Vec3::new(0.0, 0.0, orbit.distance)).looking_at(orbit.focus, Vec3::Y)
}

/// The far plane reaches the whole graph from the maximum distance.
fn projection(orbit: &OrbitCamera) -> Projection {
    let far = orbit.max_distance() + orbit.radius * 2.0;

    match orbit.mode {
        ViewMode::ThreeD => Projection::Perspective(PerspectiveProjection {
            fov: FIELD_OF_VIEW,
            near: (orbit.min_distance() * 0.1).max(0.01),
            far,
            ..default()
        }),
        ViewMode::TwoD => Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical { viewport_height: orbit.visible_height() },
            near: -far,
            far,
            ..OrthographicProjection::default_3d()
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extent_measures_from_the_centroid() {
        let (centre, radius) = extent(&[Vec3::new(-2.0, 0.0, 0.0), Vec3::new(4.0, 0.0, 0.0)]).unwrap();

        assert_eq!((centre, radius), (Vec3::new(1.0, 0.0, 0.0), 3.0));
        assert!(extent(&[]).is_none());
    }

    #[test]
    fn framing_keeps_the_whole_graph_in_view() {
        let mut orbit = OrbitCamera::default();
        orbit.frame(Vec3::ZERO, 500.0, true);

        assert!(orbit.distance * (FIELD_OF_VIEW / 2.0).sin() >= 500.0);
        assert!(orbit.max_distance() > orbit.distance && orbit.min_distance() < orbit.distance);
    }

    #[test]
    fn the_2d_view_always_faces_the_front() {
        let mut orbit = OrbitCamera { mode: ViewMode::TwoD, yaw: 1.0, pitch: 0.5, ..OrbitCamera::default() };
        orbit.frame(Vec3::ZERO, 50.0, false);

        assert_eq!((orbit.yaw, orbit.pitch), (0.0, 0.0));
        assert!(orbit_transform(&orbit).forward().as_vec3().abs_diff_eq(Vec3::NEG_Z, 1e-6));
    }
}
