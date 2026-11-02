//! Applies the icon to the window. Bevy has no API for it, so the winit window
//! is used directly.

use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::winit::WINIT_WINDOWS;
use winit::window::Icon;

/// Also the symbol of the logo in the menu bar.
pub const ICON: &[u8] = include_bytes!("../icon/GraphBeX Logo 512 transparent.png");

pub struct WindowIconPlugin;

impl Plugin for WindowIconPlugin {
    fn build(&self, app: &mut App) {
        // The winit windows only exist once the event loop runs, so this tries
        // every frame until the window shows up.
        app.add_systems(Update, apply_window_icon);
    }
}

/// winit only sets the icon from the main thread, which `NonSendMarker`
/// pins this system to.
fn apply_window_icon(_main_thread: NonSendMarker, mut applied: Local<bool>) {
    if *applied {
        return;
    }

    WINIT_WINDOWS.with_borrow(|windows| {
        if windows.windows.is_empty() {
            return;
        }
        *applied = true;

        let icon = image::load_from_memory(ICON).map_err(|error| error.to_string()).and_then(|image| {
            let image = image.into_rgba8();
            let (width, height) = image.dimensions();
            Icon::from_rgba(image.into_raw(), width, height).map_err(|error| error.to_string())
        });

        match icon {
            Ok(icon) => windows.windows.values().for_each(|window| window.set_window_icon(Some(icon.clone()))),
            Err(error) => log::error!("The application icon could not be applied: {error}."),
        }
    });
}
