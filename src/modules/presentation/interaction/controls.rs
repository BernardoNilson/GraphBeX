//! The actions behind buttons, menu entries and keyboard shortcuts.

use std::path::PathBuf;
use std::thread;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use crossbeam_channel::{Receiver, Sender, unbounded};
use rfd::{FileDialog, MessageDialog};

use super::camera::ViewRequests;
use super::status::WINDOW_TITLE;
use super::menu::{MenuItem, OpenMenu};
use super::screen::InterfaceRoot;
use super::theme::{self, Mode, Tone};
use crate::modules::application::layout_parameters::{ParameterId, ParameterValues, Setting};
use crate::modules::application::processing::{LayoutCommand, LayoutKind};
use crate::modules::data::export;
use crate::modules::data::format::GraphFormat;
use crate::modules::presentation::graph_state::{LayoutPositions, LoadedGraph, VertexColoring};

/// Where the file dialog opens: the bundled datasets.
const GRAPH_DIRECTORY: &str = "data/graph";

const CONTROLS_HELP: &str = "3D view
  Left drag: rotate
  Right drag: pan
  Mouse wheel: zoom

2D view
  Left or right drag: pan
  Mouse wheel: zoom

Left click: select a vertex, or clear the selection

Shortcuts
  Ctrl+O: open a graph
  F5: run the layout
  Home: reset the view";

/// The sending end of the command channel to the processing engine.
#[derive(Resource)]
pub struct InputChannel(pub Sender<LayoutCommand>);

/// The screenshot paths chosen in the save dialog, which runs on its own thread.
#[derive(Resource)]
pub struct ScreenshotRequests(Sender<PathBuf>, Receiver<PathBuf>);

impl Default for ScreenshotRequests {
    fn default() -> Self {
        let (sender, receiver) = unbounded();
        Self(sender, receiver)
    }
}

#[derive(Component, Clone, PartialEq, Debug)]
pub enum ControlAction {
    OpenGraph,
    ExportLayout,
    SaveScreenshot,
    Exit,
    SelectAlgorithm(LayoutKind),
    SetDimensions(usize),
    Start,
    Stop,
    ResetView,
    TogglePerformancePanel,
    ToggleDarkMode,
    ShowControls,
    ShowAbout,
    /// Moves a parameter one step up (`true`) or down.
    StepParameter(ParameterId, bool),
    ToggleAuto(ParameterId),
    /// Colors the vertices by an attribute of the file, or not at all.
    ColorBy(Option<usize>),
}

impl ControlAction {
    /// The icon beside the action in menus and buttons.
    pub fn icon(&self) -> Option<char> {
        use theme::icon::*;

        Some(match self {
            Self::OpenGraph => FOLDER_OPEN,
            Self::ExportLayout => FILE_DOWN,
            Self::SaveScreenshot => CAMERA,
            Self::Exit => LOG_OUT,
            Self::SelectAlgorithm(LayoutKind::Random) => SHUFFLE,
            Self::SelectAlgorithm(LayoutKind::Spectral) => SIGMA,
            Self::SelectAlgorithm(LayoutKind::ForceDirected) => ATOM,
            Self::Start => PLAY,
            Self::Stop => SQUARE,
            Self::ResetView => FOCUS,
            Self::ShowControls => KEYBOARD,
            Self::ShowAbout => INFO,
            _ => return None,
        })
    }
}

/// The user's choices, mirroring what was sent to the engine.
#[derive(Resource)]
pub struct UiSelection {
    pub algorithm: LayoutKind,
    pub dimensions: usize,
    pub parameters: ParameterValues,
    pub performance_panel: bool,
}

impl Default for UiSelection {
    fn default() -> Self {
        Self {
            algorithm: LayoutKind::Random,
            dimensions: 3,
            parameters: ParameterValues::default(),
            performance_panel: false,
        }
    }
}

impl UiSelection {
    /// The value the next run will use for a parameter.
    pub fn resolve(&self, id: ParameterId, loaded: &LoadedGraph) -> f64 {
        self.parameters.resolve(id, loaded.node_count(), self.dimensions)
    }

    /// Whether the option an action stands for is the active one.
    pub fn is_active(&self, action: &ControlAction) -> bool {
        match action {
            ControlAction::SelectAlgorithm(kind) => self.algorithm == *kind,
            ControlAction::SetDimensions(dimensions) => self.dimensions == *dimensions,
            ControlAction::TogglePerformancePanel => self.performance_panel,
            ControlAction::ToggleAuto(id) => self.parameters.get(*id) == Setting::Auto,
            _ => false,
        }
    }
}

#[derive(SystemParam)]
pub struct ActionContext<'w> {
    channel: Res<'w, InputChannel>,
    selection: ResMut<'w, UiSelection>,
    loaded: Res<'w, LoadedGraph>,
    positions: Res<'w, LayoutPositions>,
    screenshots: Res<'w, ScreenshotRequests>,
    view: ResMut<'w, ViewRequests>,
    coloring: ResMut<'w, VertexColoring>,
    mode: ResMut<'w, Mode>,
    exit: MessageWriter<'w, AppExit>,
}

impl ActionContext<'_> {
    pub fn perform(&mut self, action: &ControlAction) {
        match action {
            ControlAction::OpenGraph => {
                let channel = self.channel.0.clone();
                thread::spawn(move || {
                    // Absolute, because on Linux the dialog runs in the desktop portal, a process with another working directory
                    let directory = std::fs::canonicalize(GRAPH_DIRECTORY).unwrap_or_else(|_| PathBuf::from(GRAPH_DIRECTORY));
                    let dialog = FileDialog::new()
                        .set_title("Open Graph")
                        .add_filter("Graphs", &GraphFormat::supported_extensions())
                        .set_directory(directory);
                    if let Some(path) = dialog.pick_file() {
                        let _ = channel.send(LayoutCommand::LoadGraph(path));
                    }
                });
            }
            ControlAction::ExportLayout if self.positions.positions.is_empty() => {
                log::info!("There is no layout to export yet.");
            }
            ControlAction::ExportLayout => {
                let positions = self.positions.positions.clone();
                save_as("Export Layout", "layout", "csv", move |path| match export::save_layout(&path, &positions) {
                    Ok(()) => log::info!("Layout exported to '{}'.", path.display()),
                    Err(error) => log::error!("The layout could not be exported: {error}."),
                });
            }
            ControlAction::SaveScreenshot => {
                let requests = self.screenshots.0.clone();
                save_as("Save Screenshot", "screenshot", "png", move |path| {
                    let _ = requests.send(path);
                });
            }
            ControlAction::Exit => {
                self.exit.write(AppExit::Success);
            }
            ControlAction::SelectAlgorithm(kind) => {
                self.selection.algorithm = *kind;
                self.send(LayoutCommand::SelectAlgorithm(*kind));
            }
            ControlAction::SetDimensions(dimensions) => {
                self.selection.dimensions = *dimensions;
                self.send(LayoutCommand::SetDimensions(*dimensions));
            }
            ControlAction::Start => self.send(LayoutCommand::Start),
            ControlAction::Stop => self.send(LayoutCommand::Stop),
            ControlAction::ResetView => self.view.reset = true,
            ControlAction::TogglePerformancePanel => self.selection.performance_panel ^= true,
            ControlAction::ToggleDarkMode => self.mode.dark ^= true,
            ControlAction::ShowControls => message("Controls", CONTROLS_HELP.to_string()),
            ControlAction::ShowAbout => message(
                "About",
                format!("{WINDOW_TITLE}\nVersion {}\n\nBuilt with Rust, Bevy and wgpu.", env!("CARGO_PKG_VERSION")),
            ),
            ControlAction::StepParameter(id, up) => {
                let current = self.selection.resolve(*id, &self.loaded);
                self.set_parameter(*id, Setting::Manual(id.spec().step(current, *up)));
            }
            ControlAction::ToggleAuto(id) => {
                let setting = match self.selection.parameters.get(*id) {
                    // Leaving Auto keeps the current value, so the field does not jump.
                    Setting::Auto => Setting::Manual(self.selection.resolve(*id, &self.loaded)),
                    Setting::Manual(_) => Setting::Auto,
                };
                self.set_parameter(*id, setting);
            }
            ControlAction::ColorBy(attribute) => self.coloring.0 = *attribute,
        }
    }

    fn set_parameter(&mut self, id: ParameterId, setting: Setting) {
        self.selection.parameters.set(id, setting);
        let setting = self.selection.parameters.get(id);
        self.send(LayoutCommand::SetParameter(id, setting));
    }

    fn send(&self, command: LayoutCommand) {
        if self.channel.0.send(command).is_err() {
            log::error!("The processing engine is gone, the command was dropped.");
        }
    }
}

/// Asks where to save a file, forcing its extension. Native dialogs block, so
/// they run on a thread of their own.
fn save_as(title: &'static str, name: &'static str, extension: &'static str, then: impl FnOnce(PathBuf) + Send + 'static) {
    thread::spawn(move || {
        let dialog = FileDialog::new()
            .set_title(title)
            .add_filter(extension, &[extension])
            .set_file_name(format!("{name}.{extension}"));
        if let Some(path) = dialog.save_file() {
            then(path.with_extension(extension));
        }
    });
}

fn message(title: &'static str, text: String) {
    thread::spawn(move || MessageDialog::new().set_title(title).set_description(text).show());
}

pub fn dispatch_actions(
    buttons: Query<(&Interaction, &ControlAction, Has<MenuItem>), Changed<Interaction>>,
    mut open_menu: ResMut<OpenMenu>,
    mut context: ActionContext,
) {
    for (interaction, action, in_menu) in &buttons {
        if *interaction == Interaction::Pressed {
            if in_menu {
                open_menu.0 = None;
            }
            context.perform(action);
        }
    }
}

pub fn keyboard_shortcuts(keys: Res<ButtonInput<KeyCode>>, mut context: ActionContext) {
    let control = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shortcuts = [
        (control && keys.just_pressed(KeyCode::KeyO), ControlAction::OpenGraph),
        (keys.just_pressed(KeyCode::F5), ControlAction::Start),
        (keys.just_pressed(KeyCode::Home), ControlAction::ResetView),
    ];

    for (pressed, action) in shortcuts {
        if pressed {
            context.perform(&action);
        }
    }
}

/// Takes the screenshots asked for in the save dialog (FR7).
/// The interface is hidden for the captured frame, so only the graph is saved.
pub fn take_screenshots(
    mut commands: Commands,
    requests: Res<ScreenshotRequests>,
    mut interface: Query<&mut Visibility, With<InterfaceRoot>>,
) {
    for path in requests.1.try_iter() {
        interface.iter_mut().for_each(|mut visibility| *visibility = Visibility::Hidden);
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path)).observe(show_interface);
    }
}

fn show_interface(_: On<ScreenshotCaptured>, mut interface: Query<&mut Visibility, With<InterfaceRoot>>) {
    interface.iter_mut().for_each(|mut visibility| *visibility = Visibility::Inherited);
}

/// The active option takes an accent tint and border.
pub fn update_button_visuals(
    selection: Res<UiSelection>,
    mode: Res<Mode>,
    mut buttons: Query<(&Interaction, &ControlAction, &mut BackgroundColor, &mut BorderColor), Without<MenuItem>>,
) {
    for (interaction, action, mut background, mut border) in &mut buttons {
        let active = selection.is_active(action);

        background.0 = match (interaction, active) {
            (Interaction::Pressed, _) | (Interaction::Hovered, true) => mode.tint(true),
            (Interaction::Hovered, false) => mode.color(Tone::Border),
            (Interaction::None, true) => mode.tint(false),
            (Interaction::None, false) => mode.color(Tone::Surface),
        };
        *border = BorderColor::all(mode.color(if active { Tone::Accent } else { Tone::Border }));
    }
}
