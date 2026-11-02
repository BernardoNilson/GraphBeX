//! The menu bar of the mockup: File, Layout, View and Help.

use bevy::prelude::*;

use super::controls::{ControlAction, UiSelection};
use super::widgets::{self, shown};
use super::theme::{self, Fill, Mode, Stroke, Tone};
use crate::modules::application::processing::{AVAILABLE_DIMENSIONS, LayoutKind};
use crate::modules::presentation::graph_state::{LoadedGraph, VertexColoring};

/// The menus, titled by their names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuId {
    File,
    Layout,
    View,
    Help,
}

enum Entry {
    /// A label, its shortcut, its action, and whether it shows a check mark.
    Item(String, Option<&'static str>, ControlAction, bool),
    Separator,
    /// The Color By entries, rebuilt for each loaded file.
    ColorByList,
}

fn item(label: &str, shortcut: Option<&'static str>, action: ControlAction) -> Entry {
    Entry::Item(label.into(), shortcut, action, false)
}

fn check(label: impl Into<String>, action: ControlAction) -> Entry {
    Entry::Item(label.into(), None, action, true)
}

impl MenuId {
    const ALL: [MenuId; 4] = [Self::File, Self::Layout, Self::View, Self::Help];

    fn entries(self) -> Vec<Entry> {
        use ControlAction::*;

        match self {
            Self::File => vec![
                item("Open Graph", Some("Ctrl+O"), OpenGraph),
                item("Export Layout", None, ExportLayout),
                item("Save Screenshot", None, SaveScreenshot),
                Entry::Separator,
                item("Exit", None, Exit),
            ],
            Self::Layout => LayoutKind::ALL
                .map(|kind| check(kind.label(), SelectAlgorithm(kind)))
                .into_iter()
                .chain([Entry::Separator])
                .chain(AVAILABLE_DIMENSIONS.map(|dimensions| check(format!("{dimensions}D"), SetDimensions(dimensions))))
                .chain([Entry::Separator, item("Run Layout", Some("F5"), Start), item("Stop Layout", None, Stop)])
                .collect(),
            Self::View => vec![
                item("Reset View", Some("Home"), ResetView),
                Entry::Separator,
                check("Performance Panel", TogglePerformancePanel),
                check("Dark Mode", ToggleDarkMode),
                Entry::Separator,
                Entry::ColorByList,
            ],
            Self::Help => vec![item("Controls", None, ShowControls), item("About", None, ShowAbout)],
        }
    }
}

#[derive(Resource, Default)]
pub struct OpenMenu(pub Option<MenuId>);

#[derive(Component)]
pub struct MenuTitle(MenuId);

#[derive(Component)]
pub struct MenuPopup(MenuId);

#[derive(Component)]
pub struct MenuItem;

#[derive(Component)]
pub struct ColorByList;

/// The check mark of an entry, shown while its option is active.
#[derive(Component)]
pub struct MenuCheck(ControlAction);

/// The logo, then the menus.
pub fn spawn_menu_bar(screen: &mut ChildSpawnerCommands, logo: Handle<Image>) {
    let bar = Node {
        width: percent(100),
        height: px(theme::MENU_BAR_HEIGHT),
        border: UiRect::bottom(px(1)),
        ..default()
    };

    screen
        .spawn((bar, Fill(Tone::Surface), Stroke(Tone::Border), Interaction::default()))
        .with_children(|bar| {
            bar.spawn(widgets::logo(logo));
            MenuId::ALL.into_iter().for_each(|menu| spawn_menu(bar, menu));
        });
}

fn spawn_menu(bar: &mut ChildSpawnerCommands, menu: MenuId) {
    let title = Node { padding: UiRect::horizontal(px(10)), align_items: AlignItems::Center, ..default() };
    let popup = Node {
        display: Display::None,
        position_type: PositionType::Absolute,
        top: percent(100),
        width: px(theme::POPUP_WIDTH),
        flex_direction: FlexDirection::Column,
        padding: UiRect::vertical(px(4)),
        border: UiRect::all(px(1)),
        ..default()
    };

    bar.spawn((Button, MenuTitle(menu), title, BackgroundColor(Color::NONE))).with_children(|title| {
        title.spawn(widgets::text(format!("{menu:?}"), theme::FONT_BODY, Tone::Text));
        title
            .spawn((
                MenuPopup(menu),
                Interaction::default(),
                popup,
                Fill(Tone::Surface),
                Stroke(Tone::Border),
                GlobalZIndex(theme::POPUP_LAYER),
            ))
            .with_children(|popup| menu.entries().into_iter().for_each(|entry| spawn_entry(popup, entry)));
    });
}

fn spawn_entry(popup: &mut ChildSpawnerCommands, entry: Entry) {
    let (label, shortcut, action, checkable) = match entry {
        Entry::Item(label, shortcut, action, checkable) => (label, shortcut, action, checkable),
        Entry::Separator => {
            popup.spawn(widgets::separator(true));
            return;
        }
        Entry::ColorByList => {
            let heading = Node { margin: UiRect::left(px(32)), ..default() };
            popup.spawn((widgets::text("Color By", theme::FONT_SMALL, Tone::TextMuted), heading));
            popup.spawn((ColorByList, Node { flex_direction: FlexDirection::Column, ..default() }));
            return;
        }
    };

    let row = Node {
        height: px(26),
        padding: UiRect::horizontal(px(8)),
        align_items: AlignItems::Center,
        column_gap: px(8),
        ..default()
    };

    popup.spawn((Button, MenuItem, action.clone(), row, BackgroundColor(Color::NONE))).with_children(|item| {
        // Every entry keeps the column of the icon or check mark, so the labels line up.
        let column = Node { width: px(16), justify_content: JustifyContent::Center, ..default() };
        item.spawn(column).with_children(|column| match (checkable, action.icon()) {
            (true, _) => {
                column.spawn((widgets::check_mark(), MenuCheck(action), Visibility::Hidden));
            }
            (false, Some(icon)) => {
                column.spawn(widgets::icon(icon, theme::FONT_ICON, Tone::TextMuted));
            }
            (false, None) => (),
        });
        item.spawn((widgets::text(label, theme::FONT_BODY, Tone::Text), Node { flex_grow: 1.0, ..default() }));
        if let Some(shortcut) = shortcut {
            item.spawn(widgets::text(shortcut, theme::FONT_SMALL, Tone::TextMuted));
        }
    });
}

/// Lists "None" and every vertex attribute of the loaded file.
pub fn rebuild_color_by_entries(mut commands: Commands, loaded: Res<LoadedGraph>, lists: Query<Entity, With<ColorByList>>) {
    for list in &lists {
        commands.entity(list).despawn_children().with_children(|list| {
            spawn_entry(list, check("None", ControlAction::ColorBy(None)));
            for (index, attribute) in loaded.attributes.0.iter().enumerate() {
                spawn_entry(list, check(attribute.name.clone(), ControlAction::ColorBy(Some(index))));
            }
        });
    }
}

/// Opens a menu on click and, while one is open, follows the pointer across
/// the titles, as desktop menu bars do.
pub fn operate_menus(titles: Query<(&Interaction, &MenuTitle), Changed<Interaction>>, mut open: ResMut<OpenMenu>) {
    for (interaction, title) in &titles {
        match interaction {
            Interaction::Pressed if open.0 == Some(title.0) => open.0 = None,
            Interaction::Pressed => open.0 = Some(title.0),
            Interaction::Hovered if open.0.is_some() => open.0 = Some(title.0),
            _ => (),
        }
    }
}

/// Closes the open menu on Escape or on a click outside it.
pub fn close_menus(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    menu_parts: Query<&Interaction, Or<(With<MenuTitle>, With<MenuPopup>)>>,
    mut open: ResMut<OpenMenu>,
) {
    let clicked_outside = mouse.any_just_pressed([MouseButton::Left, MouseButton::Right])
        && menu_parts.iter().all(|interaction| *interaction == Interaction::None);

    if open.0.is_some() && (keys.just_pressed(KeyCode::Escape) || clicked_outside) {
        open.0 = None;
    }
}

pub fn show_open_menu(open: Res<OpenMenu>, mut popups: Query<(&MenuPopup, &mut Node)>) {
    for (popup, mut node) in &mut popups {
        node.display = if open.0 == Some(popup.0) { Display::Flex } else { Display::None };
    }
}

pub fn update_menu_visuals(
    open: Res<OpenMenu>,
    selection: Res<UiSelection>,
    coloring: Res<VertexColoring>,
    mode: Res<Mode>,
    mut titles: Query<(&Interaction, &MenuTitle, &mut BackgroundColor), Without<MenuItem>>,
    mut items: Query<(&Interaction, &mut BackgroundColor), With<MenuItem>>,
    mut checks: Query<(&MenuCheck, &mut Visibility)>,
) {
    for (interaction, title, mut color) in &mut titles {
        let highlighted = open.0 == Some(title.0) || *interaction != Interaction::None;
        color.0 = if highlighted { mode.color(Tone::Border) } else { Color::NONE };
    }
    for (interaction, mut color) in &mut items {
        color.0 = if *interaction == Interaction::None { Color::NONE } else { mode.tint(false) };
    }
    for (check, mut visibility) in &mut checks {
        *visibility = shown(match &check.0 {
            ControlAction::ColorBy(attribute) => coloring.0 == *attribute,
            ControlAction::ToggleDarkMode => mode.dark,
            action => selection.is_active(action),
        });
    }
}
