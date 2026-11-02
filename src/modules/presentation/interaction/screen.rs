//! The screen of the mockup: the menu bar, the canvas, the panel on the right
//! and the status bar.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use image::imageops::FilterType;

use super::controls::ControlAction;
use super::parameters::ParameterList;
use super::theme::{self, Fill, Stroke, Tone};
use super::{menu, widgets};
use crate::modules::application::processing::{AVAILABLE_DIMENSIONS, LayoutKind};
use crate::modules::presentation::window_icon;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum StatusText {
    Graph,
    Layout,
    Cpu,
    Ram,
    Vram,
    Fps,
    Iterations,
    Selection,
    PerformanceLabels,
    PerformanceValues,
}

/// The whole interface, hidden while a screenshot is taken.
#[derive(Component)]
pub struct InterfaceRoot;

/// The hint on the canvas while no graph is loaded.
#[derive(Component)]
pub struct EmptyCanvasHint;

/// The panel toggled from the View menu.
#[derive(Component)]
pub struct PerformancePanel;

pub fn spawn_interface(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let screen = Node { width: percent(100), height: percent(100), flex_direction: FlexDirection::Column, ..default() };
    let logo = images.add(logo_symbol());

    commands.spawn((InterfaceRoot, screen)).with_children(|screen| {
        menu::spawn_menu_bar(screen, logo);
        screen.spawn(Node { width: percent(100), flex_grow: 1.0, ..default() }).with_children(|workspace| {
            spawn_canvas(workspace);
            spawn_side_panel(workspace);
        });
        spawn_status_bar(screen);
    });
}

/// The icon, reduced on the CPU so it stays sharp at the size of the menu bar.
fn logo_symbol() -> Image {
    let icon = image::load_from_memory(window_icon::ICON).expect("the bundled icon is valid");
    Image::from_dynamic(icon.resize(40, 40, FilterType::Lanczos3), true, RenderAssetUsages::RENDER_WORLD)
}

/// The canvas has no background: the graph is drawn behind the interface.
fn spawn_canvas(workspace: &mut ChildSpawnerCommands) {
    let canvas = Node {
        flex_grow: 1.0,
        height: percent(100),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    };
    let panel = Node {
        position_type: PositionType::Absolute,
        left: px(8),
        bottom: px(8),
        padding: UiRect::all(px(8)),
        column_gap: px(12),
        border: UiRect::all(px(1)),
        ..default()
    };

    workspace.spawn(canvas).with_children(|canvas| {
        let hint = "File > Open Graph (Ctrl+O) to load a graph";
        canvas.spawn((widgets::text(hint, theme::FONT_BODY, Tone::TextMuted), EmptyCanvasHint));
        canvas.spawn((
            PerformancePanel,
            Interaction::default(),
            Visibility::Hidden,
            panel,
            Fill(Tone::Surface),
            Stroke(Tone::Border),
            // A column of labels and one of values, so the rows line up.
            children![
                (widgets::text("", theme::FONT_SMALL, Tone::TextMuted), StatusText::PerformanceLabels),
                (widgets::text("", theme::FONT_SMALL, Tone::Text), StatusText::PerformanceValues),
            ],
        ));
    });
}

fn spawn_side_panel(workspace: &mut ChildSpawnerCommands) {
    let panel = Node {
        width: px(theme::SIDE_PANEL_WIDTH),
        height: percent(100),
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(px(theme::PANEL_PADDING)),
        row_gap: px(theme::SECTION_GAP),
        border: UiRect::left(px(1)),
        ..default()
    };

    workspace
        .spawn((panel, Fill(Tone::Surface), Stroke(Tone::Border), Interaction::default()))
        .with_children(|panel| {
            panel.spawn(widgets::section("Layout")).with_children(|section| {
                for kind in LayoutKind::ALL {
                    let action = ControlAction::SelectAlgorithm(kind);
                    section.spawn((widgets::button(action.icon(), kind.label()), action));
                }
            });

            panel.spawn(widgets::section("Parameters")).with_children(|section| {
                section.spawn(widgets::text("Dimensions", theme::FONT_SMALL, Tone::TextMuted));
                section.spawn(widgets::row()).with_children(|row| {
                    for dimensions in AVAILABLE_DIMENSIONS {
                        row.spawn((widgets::button(None, format!("{dimensions}D")), ControlAction::SetDimensions(dimensions)));
                    }
                });
                let list = Node { width: percent(100), flex_direction: FlexDirection::Column, row_gap: px(8), ..default() };
                section.spawn((ParameterList, list));
            });

            panel.spawn(widgets::row()).with_children(|row| {
                for (label, action) in [("Run Layout", ControlAction::Start), ("Stop", ControlAction::Stop)] {
                    row.spawn((widgets::button(action.icon(), label), action));
                }
            });

            panel
                .spawn(widgets::section("Selection"))
                .with_child((widgets::text("", theme::FONT_SMALL, Tone::Text), StatusText::Selection));
        });
}

/// The graph and the layout on the left, the metrics on the right.
fn spawn_status_bar(screen: &mut ChildSpawnerCommands) {
    let bar = Node {
        width: percent(100),
        height: px(theme::STATUS_BAR_HEIGHT),
        padding: UiRect::horizontal(px(10)),
        align_items: AlignItems::Center,
        column_gap: px(10),
        border: UiRect::top(px(1)),
        ..default()
    };
    let status = |kind| (widgets::text("", theme::FONT_SMALL, Tone::TextMuted), kind);

    screen
        .spawn((bar, Fill(Tone::Surface), Stroke(Tone::Border), Interaction::default()))
        .with_children(|bar| {
            bar.spawn(status(StatusText::Graph));
            bar.spawn(widgets::separator(false));
            bar.spawn(status(StatusText::Layout));
            bar.spawn(Node { flex_grow: 1.0, ..default() });

            let metrics = [StatusText::Cpu, StatusText::Ram, StatusText::Vram, StatusText::Fps, StatusText::Iterations];
            for (index, metric) in metrics.into_iter().enumerate() {
                if index > 0 {
                    bar.spawn(widgets::separator(false));
                }
                bar.spawn((widgets::figures(theme::FONT_SMALL, Tone::TextMuted), metric));
            }
        });
}
