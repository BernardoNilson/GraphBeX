//! The parameter fields of the selected algorithm (FR2), built from the specs,
//! so a new algorithm needs no interface code.

use bevy::prelude::*;

use super::controls::{ControlAction, UiSelection};
use super::widgets::{self, shown};
use super::theme::{self, Fill, Stroke, Tone};
use crate::modules::application::layout_parameters::{ParameterId, Stepping};
use crate::modules::presentation::graph_state::LoadedGraph;

/// The container the fields are rebuilt into when the algorithm changes.
#[derive(Component)]
pub struct ParameterList;

#[derive(Component)]
pub struct ParameterValue(ParameterId);

#[derive(Component)]
pub struct AutoMark(ParameterId);

pub fn rebuild_parameter_fields(
    mut commands: Commands,
    selection: Res<UiSelection>,
    lists: Query<Entity, With<ParameterList>>,
) {
    for list in &lists {
        commands.entity(list).despawn_children().with_children(|list| {
            selection.algorithm.parameters().iter().for_each(|&id| spawn_field(list, id));
        });
    }
}

/// The label, an Auto check box when the value can follow the graph, and the
/// value between − and +.
fn spawn_field(list: &mut ChildSpawnerCommands, id: ParameterId) {
    let spec = id.spec();
    let field = Node { width: percent(100), flex_direction: FlexDirection::Column, row_gap: px(3), ..default() };
    let value_box = Node {
        flex_grow: 1.0,
        height: px(theme::CONTROL_HEIGHT),
        padding: UiRect::horizontal(px(6)),
        align_items: AlignItems::Center,
        justify_content: JustifyContent::FlexEnd,
        border: UiRect::all(px(1)),
        border_radius: BorderRadius::all(px(theme::CORNER_RADIUS)),
        ..default()
    };

    list.spawn(field).with_children(|field| {
        field.spawn(widgets::row()).with_children(|header| {
            header.spawn((widgets::text(spec.label, theme::FONT_SMALL, Tone::TextMuted), Node { flex_grow: 1.0, ..default() }));

            if spec.automatic.is_some() {
                let toggle = Node { align_items: AlignItems::Center, column_gap: px(5), ..default() };
                header.spawn((Button, ControlAction::ToggleAuto(id), toggle)).with_children(|toggle| {
                    toggle.spawn(widgets::check_box()).with_children(|check_box| {
                        check_box.spawn((widgets::check_mark(), AutoMark(id)));
                    });
                    toggle.spawn(widgets::text("Auto", theme::FONT_SMALL, Tone::TextMuted));
                });
            }
        });

        field.spawn(widgets::row()).with_children(|controls| {
            controls.spawn((widgets::small_button(theme::icon::MINUS), ControlAction::StepParameter(id, false)));
            controls
                .spawn((value_box, Fill(Tone::Background), Stroke(Tone::Border)))
                .with_child((widgets::text("", theme::FONT_BODY, Tone::Text), ParameterValue(id)));
            controls.spawn((widgets::small_button(theme::icon::PLUS), ControlAction::StepParameter(id, true)));
        });
    });
}

pub fn refresh_parameter_fields(
    selection: Res<UiSelection>,
    loaded: Res<LoadedGraph>,
    mut values: Query<(&ParameterValue, &mut Text)>,
    mut marks: Query<(&AutoMark, &mut Visibility)>,
) {
    for (value, mut text) in &mut values {
        **text = format_value(value.0, selection.resolve(value.0, &loaded));
    }
    for (mark, mut visibility) in &mut marks {
        *visibility = shown(selection.is_active(&ControlAction::ToggleAuto(mark.0)));
    }
}

fn format_value(id: ParameterId, value: f64) -> String {
    match id.spec().stepping {
        Stepping::Integer(_) => format!("{value:.0}"),
        Stepping::Factor(_) if value != 0.0 && value.abs() < 1e-3 => format!("{value:.0e}"),
        Stepping::Factor(_) => format!("{value:.1}"),
        Stepping::Decimal(_) => format!("{value:.2}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_formatted_by_kind() {
        assert_eq!(format_value(ParameterId::Seed, 42.0), "42");
        assert_eq!(format_value(ParameterId::Bounds, 585.43), "585.4");
        assert_eq!(format_value(ParameterId::ConvergenceThreshold, 1e-9), "1e-9");
        assert_eq!(format_value(ParameterId::Theta, 0.8), "0.80");
    }
}
