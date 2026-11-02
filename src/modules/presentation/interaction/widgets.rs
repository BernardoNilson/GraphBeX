//! The building blocks of the interface, each returned as a bundle.

use bevy::prelude::*;
use bevy::text::FontFeatureTag;

use super::theme::{self, Fill, Ink, Stroke, Tone};

/// A push button filling its row, with an optional icon. Buttons sharing a
/// row split it.
pub fn button(icon: Option<char>, label: impl Into<String>) -> impl Bundle {
    framed_button(percent(100), icon, label.into())
}

/// A square button with an icon, for the minus and plus of a numeric field.
pub fn small_button(icon: char) -> impl Bundle {
    framed_button(px(theme::CONTROL_HEIGHT), Some(icon), String::new())
}

/// Its colors come from `update_button_visuals`.
fn framed_button(width: Val, icon: Option<char>, label: String) -> impl Bundle {
    // The icon only keeps its distance from a label.
    let gap = if icon.is_some() && !label.is_empty() { 6.0 } else { 0.0 };
    let glyph = icon.map(String::from).unwrap_or_default();

    (
        Button,
        Node {
            width,
            height: px(theme::CONTROL_HEIGHT),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(theme::CORNER_RADIUS)),
            ..default()
        },
        children![
            (icon_text(glyph, theme::FONT_ICON, Tone::TextMuted), Node { margin: UiRect::right(px(gap)), ..default() }),
            text(label, theme::FONT_BODY, Tone::Text),
        ],
    )
}

/// A semibold heading, set off by a rule beneath it, followed by its content.
pub fn section(title: impl Into<String>) -> impl Bundle {
    let heading = (
        Node { width: percent(100), padding: UiRect::bottom(px(4)), border: UiRect::bottom(px(1)), ..default() },
        Stroke(Tone::Border),
        children![strong(title, theme::FONT_SMALL, Tone::Text)],
    );

    (
        Node { width: percent(100), flex_direction: FlexDirection::Column, row_gap: px(theme::ITEM_GAP), ..default() },
        children![heading],
    )
}

/// The GraphBeX logo: the symbol and the name, with the X in the accent color.
pub fn logo(symbol: Handle<Image>) -> impl Bundle {
    let size = 15.0;
    let x = (TextSpan::new("X"), TextFont { font: theme::SEMIBOLD, font_size: size, ..default() }, Ink(Tone::Accent));

    (
        Node { align_items: AlignItems::Center, column_gap: px(6), padding: UiRect::horizontal(px(12)), ..default() },
        children![
            (ImageNode::new(symbol), Node { width: px(20), height: px(20), ..default() }),
            (strong("GraphBe", size, Tone::Brand), children![x]),
        ],
    )
}

pub fn row() -> Node {
    Node { width: percent(100), align_items: AlignItems::Center, column_gap: px(theme::ITEM_GAP), ..default() }
}

/// A line separating groups: horizontal across a menu, or a short vertical
/// one between the fields of the status bar.
pub fn separator(horizontal: bool) -> impl Bundle {
    let node = match horizontal {
        true => Node { width: percent(100), height: px(1), margin: UiRect::vertical(px(4)), ..default() },
        false => Node { width: px(1), height: px(14), ..default() },
    };
    (node, Fill(Tone::Border))
}

/// A framed square for a check mark.
pub fn check_box() -> impl Bundle {
    (
        Node {
            width: px(15),
            height: px(15),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(3)),
            ..default()
        },
        Fill(Tone::Surface),
        Stroke(Tone::Border),
    )
}

/// The mark of a check box or of a checked menu entry.
pub fn check_mark() -> impl Bundle {
    icon(theme::icon::CHECK, theme::FONT_SMALL, Tone::Accent)
}

/// A Lucide icon, `glyph` being one of `theme::icon`.
pub fn icon(glyph: char, size: f32, tone: Tone) -> impl Bundle {
    icon_text(glyph.into(), size, tone)
}

fn icon_text(content: String, size: f32, tone: Tone) -> impl Bundle {
    (Text::new(content), TextFont { font: theme::ICONS, font_size: size, ..default() }, Ink(tone))
}

pub fn text(content: impl Into<String>, size: f32, tone: Tone) -> impl Bundle {
    (Text::new(content), TextFont { font_size: size, ..default() }, Ink(tone))
}

pub fn strong(content: impl Into<String>, size: f32, tone: Tone) -> impl Bundle {
    (Text::new(content), TextFont { font: theme::SEMIBOLD, font_size: size, ..default() }, Ink(tone))
}

/// A text for changing numbers. Tabular figures give every digit the same
/// width, so the text after it does not shift. They also widen the hyphen,
/// so they are not used everywhere.
pub fn figures(size: f32, tone: Tone) -> impl Bundle {
    let font_features = [FontFeatureTag::TABULAR_FIGURES].into();
    (Text::default(), TextFont { font_size: size, font_features, ..default() }, Ink(tone))
}

/// Shown or hidden, keeping the visibility of the parent.
pub fn shown(visible: bool) -> Visibility {
    if visible { Visibility::Inherited } else { Visibility::Hidden }
}
