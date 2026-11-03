use std::sync::Arc;

use eframe::egui;

use crate::settings::Theme;

/// Font family name for the bold Atkinson Hyperlegible face.
pub const BOLD_FAMILY: &str = "bold";

pub struct ThemeColors {
    // Surfaces
    pub bg: egui::Color32,
    pub bar: egui::Color32,
    pub pane: egui::Color32,
    pub pane_active: egui::Color32,
    pub chip: egui::Color32,
    pub field: egui::Color32,
    pub line: egui::Color32,
    pub scrim: egui::Color32,

    // Text
    pub text: egui::Color32,
    pub muted: egui::Color32,
    pub faint: egui::Color32,

    // Accent and focus
    pub accent: egui::Color32,
    pub on_accent: egui::Color32,
    pub focused_fill: egui::Color32,
    pub focused_stroke: egui::Color32,

    // Marked rows
    pub mark: egui::Color32,
    pub mark_fill: egui::Color32,

    // Destructive actions and errors
    pub danger: egui::Color32,
    pub danger_fill: egui::Color32,

    // On-screen keyboard
    pub key: egui::Color32,
    pub key_fn: egui::Color32,
}

impl ThemeColors {
    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self::dark(),
            Theme::Light => Self::light(),
        }
    }

    fn dark() -> Self {
        Self {
            bg: egui::Color32::from_rgb(16, 18, 20),
            bar: egui::Color32::from_rgb(21, 24, 26),
            pane: egui::Color32::from_rgb(23, 26, 29),
            pane_active: egui::Color32::from_rgb(27, 31, 34),
            chip: egui::Color32::from_rgb(35, 40, 43),
            field: egui::Color32::from_rgb(18, 21, 23),
            line: egui::Color32::from_rgb(43, 48, 52),
            scrim: egui::Color32::from_rgba_unmultiplied(8, 10, 11, 175),
            text: egui::Color32::from_rgb(236, 239, 237),
            muted: egui::Color32::from_rgb(167, 176, 172),
            faint: egui::Color32::from_rgb(139, 149, 145),
            accent: egui::Color32::from_rgb(140, 203, 166),
            on_accent: egui::Color32::from_rgb(13, 31, 22),
            focused_fill: egui::Color32::from_rgb(35, 69, 58),
            focused_stroke: egui::Color32::from_rgb(182, 232, 202),
            mark: egui::Color32::from_rgb(233, 188, 82),
            mark_fill: egui::Color32::from_rgb(58, 50, 29),
            danger: egui::Color32::from_rgb(242, 154, 128),
            danger_fill: egui::Color32::from_rgb(51, 35, 31),
            key: egui::Color32::from_rgb(42, 48, 52),
            key_fn: egui::Color32::from_rgb(35, 40, 43),
        }
    }

    fn light() -> Self {
        Self {
            bg: egui::Color32::from_rgb(230, 234, 236),
            bar: egui::Color32::from_rgb(246, 247, 248),
            pane: egui::Color32::from_rgb(241, 243, 244),
            pane_active: egui::Color32::WHITE,
            chip: egui::Color32::from_rgb(229, 233, 235),
            field: egui::Color32::from_rgb(246, 247, 248),
            line: egui::Color32::from_rgb(210, 216, 220),
            scrim: egui::Color32::from_rgba_unmultiplied(20, 24, 26, 120),
            text: egui::Color32::from_rgb(20, 24, 26),
            muted: egui::Color32::from_rgb(75, 85, 83),
            faint: egui::Color32::from_rgb(93, 103, 100),
            accent: egui::Color32::from_rgb(29, 116, 72),
            on_accent: egui::Color32::WHITE,
            focused_fill: egui::Color32::from_rgb(210, 238, 221),
            focused_stroke: egui::Color32::from_rgb(29, 116, 72),
            mark: egui::Color32::from_rgb(125, 83, 0),
            mark_fill: egui::Color32::from_rgb(246, 229, 186),
            danger: egui::Color32::from_rgb(178, 52, 30),
            danger_fill: egui::Color32::from_rgb(250, 228, 222),
            key: egui::Color32::from_rgb(229, 233, 235),
            key_fn: egui::Color32::from_rgb(214, 220, 224),
        }
    }
}

/// Installs Atkinson Hyperlegible as the proportional font, with a bold face
/// available through [`bold`].
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "atkinson".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/AtkinsonHyperlegible-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "atkinson_bold".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/AtkinsonHyperlegible-Bold.ttf"
        ))),
    );

    let proportional = fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default();
    proportional.insert(0, "atkinson".to_owned());
    let fallbacks = proportional.clone();

    let mut bold_family = vec!["atkinson_bold".to_owned()];
    bold_family.extend(fallbacks);
    fonts
        .families
        .insert(egui::FontFamily::Name(BOLD_FAMILY.into()), bold_family);

    ctx.set_fonts(fonts);
}

pub fn configure_text_styles(style: &mut egui::Style) {
    use egui::{FontFamily, FontId, TextStyle};

    style.text_styles = [
        (
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(14.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(13.0, FontFamily::Monospace),
        ),
        (
            TextStyle::Heading,
            FontId::new(20.0, FontFamily::Name(BOLD_FAMILY.into())),
        ),
    ]
    .into();
}

pub fn regular(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Proportional)
}

pub fn bold(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(BOLD_FAMILY.into()))
}

pub fn mono(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Monospace)
}
