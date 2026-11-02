pub mod camera;
pub mod controls;
pub mod menu;
pub mod parameters;
pub mod screen;
pub mod selection;
pub mod status;
pub mod theme;
pub mod widgets;

use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use crossbeam_channel::Sender;

use crate::modules::application::processing::{LayoutCommand, LayoutKind};
use crate::modules::presentation::graph_state::LoadedGraph;
use controls::{InputChannel, ScreenshotRequests, UiSelection};

/// Builds the interface and turns what the user does into commands for the
/// processing engine.
pub struct Interaction(pub Sender<LayoutCommand>);

impl Plugin for Interaction {
    fn build(&self, app: &mut App) {
        theme::install_fonts(&mut app.world_mut().resource_mut::<Assets<Font>>());

        let input = (
            selection::select_vertex,
            menu::operate_menus,
            menu::close_menus,
            controls::dispatch_actions,
            controls::keyboard_shortcuts,
        );
        let view = (camera::frame_graph, camera::control_camera, camera::apply_camera);
        let refresh = (
            menu::show_open_menu.run_if(resource_changed::<menu::OpenMenu>),
            menu::update_menu_visuals,
            menu::rebuild_color_by_entries.run_if(resource_changed::<LoadedGraph>),
            controls::update_button_visuals,
            controls::take_screenshots,
            parameters::rebuild_parameter_fields.run_if(algorithm_changed),
            parameters::refresh_parameter_fields,
            status::refresh_status_texts,
            status::update_window_title.run_if(resource_changed::<LoadedGraph>),
            status::update_canvas_overlays,
        );

        app.insert_resource(InputChannel(self.0.clone()))
            .init_resource::<UiSelection>()
            .init_resource::<menu::OpenMenu>()
            .init_resource::<camera::ViewRequests>()
            .init_resource::<ScreenshotRequests>()
            .add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_systems(Startup, (theme::follow_system, camera::spawn_camera, screen::spawn_interface))
            .add_systems(Update, input.chain())
            .add_systems(Update, (view.chain(), refresh).after(controls::keyboard_shortcuts))
            .add_systems(Update, theme::paint.after(status::refresh_status_texts));
    }
}

/// True on the first frame and whenever another algorithm is selected, so the
/// parameter fields always match it.
fn algorithm_changed(selection: Res<UiSelection>, mut last: Local<Option<LayoutKind>>) -> bool {
    let changed = *last != Some(selection.algorithm);
    *last = Some(selection.algorithm);
    changed
}
