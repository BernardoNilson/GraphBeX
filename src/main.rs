use bevy::prelude::*;
use log::LevelFilter;

mod modules;

use modules::application::{monitoring, processing};
use modules::presentation::graph_state::GraphStatePlugin;
use modules::presentation::interaction::{self, status::WINDOW_TITLE};
use modules::presentation::metrics::MetricsPlugin;
use modules::presentation::rendering::Rendering;
use modules::presentation::window_icon::WindowIconPlugin;

/// The processing and monitoring engines run on threads of their own. Bevy
/// takes the main thread until the window closes, talking to them through
/// channels.
fn main() {
    // The Vulkan loader of wgpu floods the log with setup details.
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .filter_module("wgpu_hal", LevelFilter::Error)
        .init();
    log::info!("{WINDOW_TITLE} is starting.");

    let (commands, updates) = processing::start();
    let hardware = monitoring::start();

    // env_logger is the global logger, so Bevy's own is disabled.
    let window = Window { title: WINDOW_TITLE.to_string(), ..default() };
    let defaults = DefaultPlugins
        .build()
        .disable::<bevy::log::LogPlugin>()
        .set(WindowPlugin { primary_window: Some(window), ..default() });

    App::new()
        .add_plugins(defaults)
        .add_plugins((
            GraphStatePlugin(updates),
            MetricsPlugin(hardware),
            WindowIconPlugin,
            Rendering,
            interaction::Interaction(commands),
        ))
        .run();

    log::info!("Application has shut down.");
}
