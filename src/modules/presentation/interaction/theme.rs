//! The look of GraphBeX: its palette, fonts, icons and measurements.

use bevy::asset::{AssetId, uuid_handle};
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowTheme};

/// The light or dark mode. It starts as the system's and is not saved.
#[derive(Resource)]
pub struct Mode {
    pub dark: bool,
}

/// What a color is for. Each mode has its own color for it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Accent,
    Background,
    Surface,
    Border,
    Text,
    TextMuted,
    /// Outside the palettes.
    Error,
    /// The name in the logo.
    Brand,
    Vertex,
    SelectedVertex,
    NeighborVertex,
    Edge,
}

impl Mode {
    pub fn color(&self, tone: Tone) -> Color {
        let (light, dark) = match tone {
            Tone::Accent => (0x3B82F6, 0x60A5FA),
            Tone::Background => (0xF8FAFC, 0x0F172A),
            Tone::Surface => (0xFFFFFF, 0x1E293B),
            Tone::Border => (0xE2E8F0, 0x334155),
            Tone::Text => (0x1F2937, 0xF8FAFC),
            Tone::TextMuted => (0x64748B, 0x94A3B8),
            Tone::Error => (0xDC2626, 0xF87171),
            Tone::Brand => (0x475569, 0xF8FAFC),
            Tone::Vertex => (0x93C5FD, 0x3B82F6),
            Tone::SelectedVertex => (0xFBBF24, 0xFBBF24),
            Tone::NeighborVertex => (0x475569, 0x94A3B8),
            Tone::Edge => (0x64748B, 0x334155),
        };
        let hex: u32 = if self.dark { dark } else { light };
        Color::srgb_u8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
    }

    /// The translucent accent of the active option and of the entry under the pointer.
    pub fn tint(&self, strong: bool) -> Color {
        self.color(Tone::Accent).with_alpha(if strong { 0.28 } else { 0.14 })
    }
}

/// The tones of a node, painted by `paint`.
#[derive(Component, Clone, Copy, PartialEq)]
#[require(BackgroundColor)]
pub struct Fill(pub Tone);

#[derive(Component, Clone, Copy, PartialEq)]
#[require(BorderColor)]
pub struct Stroke(pub Tone);

#[derive(Component, Clone, Copy, PartialEq)]
#[require(TextColor)]
pub struct Ink(pub Tone);

/// Starts in the mode of the system, which Bevy reads when it creates the window.
pub fn follow_system(mut commands: Commands, windows: Query<&Window, With<PrimaryWindow>>) {
    let dark = windows.iter().any(|window| window.window_theme == Some(WindowTheme::Dark));
    commands.insert_resource(Mode { dark });
}

/// Colors each node by its tones when it appears or changes, and every node
/// when the mode changes. The title bar of the window follows too.
pub fn paint(
    mode: Res<Mode>,
    mut fills: Query<(Ref<Fill>, &mut BackgroundColor)>,
    mut strokes: Query<(Ref<Stroke>, &mut BorderColor)>,
    mut inks: Query<(Ref<Ink>, &mut TextColor)>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let all = mode.is_changed();
    for (fill, mut color) in &mut fills {
        if all || fill.is_changed() {
            color.0 = mode.color(fill.0);
        }
    }
    for (stroke, mut color) in &mut strokes {
        if all || stroke.is_changed() {
            *color = BorderColor::all(mode.color(stroke.0));
        }
    }
    for (ink, mut color) in &mut inks {
        if all || ink.is_changed() {
            color.0 = mode.color(ink.0);
        }
    }
    if all {
        let theme = if mode.dark { WindowTheme::Dark } else { WindowTheme::Light };
        windows.iter_mut().for_each(|mut window| window.window_theme = Some(theme));
    }
}

pub const FONT_BODY: f32 = 14.0;
pub const FONT_SMALL: f32 = 13.0;
pub const FONT_ICON: f32 = 15.0;

pub const MENU_BAR_HEIGHT: f32 = 32.0;
pub const STATUS_BAR_HEIGHT: f32 = 26.0;
pub const SIDE_PANEL_WIDTH: f32 = 280.0;
pub const PANEL_PADDING: f32 = 12.0;
pub const SECTION_GAP: f32 = 16.0;
pub const ITEM_GAP: f32 = 6.0;
pub const CONTROL_HEIGHT: f32 = 26.0;
pub const CORNER_RADIUS: f32 = 4.0;
pub const POPUP_WIDTH: f32 = 240.0;

/// Popups float above the rest of the interface.
pub const POPUP_LAYER: i32 = 100;

pub const SEMIBOLD: Handle<Font> = uuid_handle!("8c1d6f3a-2b4e-4d7a-9f10-5e6a7b8c9d01");
pub const ICONS: Handle<Font> = uuid_handle!("8c1d6f3a-2b4e-4d7a-9f10-5e6a7b8c9d02");

/// Inter for the text, its semibold for headings and the logo, and Lucide for
/// the icons. Inter Medium takes the place of Bevy's default font, so every
/// text uses it unless told otherwise. The regular weight looked too thin.
pub fn install_fonts(fonts: &mut Assets<Font>) {
    let faces: [(AssetId<Font>, &[u8]); 3] = [
        (AssetId::default(), include_bytes!("../../font/Inter-Medium.ttf")),
        (SEMIBOLD.id(), include_bytes!("../../font/Inter-SemiBold.ttf")),
        (ICONS.id(), include_bytes!("../../font/Lucide.ttf")),
    ];

    for (id, bytes) in faces {
        let font = Font::try_from_bytes(bytes.to_vec()).expect("the bundled fonts are valid");
        fonts.insert(id, font).expect("the font ids are valid");
    }
}

/// Glyphs of the Lucide font (lucide-static 1.61.0).
pub mod icon {
    pub const ATOM: char = '\u{e3d7}';
    pub const CAMERA: char = '\u{e064}';
    pub const CHECK: char = '\u{e06c}';
    pub const FILE_DOWN: char = '\u{e318}';
    pub const FOCUS: char = '\u{e29e}';
    pub const FOLDER_OPEN: char = '\u{e247}';
    pub const INFO: char = '\u{e0f9}';
    pub const KEYBOARD: char = '\u{e284}';
    pub const LOG_OUT: char = '\u{e10e}';
    pub const MINUS: char = '\u{e11c}';
    pub const PLAY: char = '\u{e13c}';
    pub const PLUS: char = '\u{e13d}';
    pub const SHUFFLE: char = '\u{e15e}';
    pub const SIGMA: char = '\u{e201}';
    pub const SQUARE: char = '\u{e167}';
}
