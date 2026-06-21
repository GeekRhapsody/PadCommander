use eframe::egui;

use crate::settings::Theme;

pub struct ThemeColors {
    // Egui-level visuals
    pub panel_fill: egui::Color32,
    pub window_fill: egui::Color32,
    pub faint_bg: egui::Color32,
    pub selection_bg: egui::Color32,
    pub accent: egui::Color32,
    pub text: egui::Color32,

    // Panel backgrounds
    pub top_bottom_panel: egui::Color32,
    pub central_panel: egui::Color32,

    // Modal / overlay
    pub modal_fill: egui::Color32,
    pub modal_border: egui::Color32,
    pub input_help_fill: egui::Color32,
    pub text_input_fill: egui::Color32,
    pub text_input_field_fill: egui::Color32,
    pub text_input_field_border: egui::Color32,

    // Interactive elements
    pub focused_fill: egui::Color32,
    pub focused_stroke: egui::Color32,

    // Pane
    pub pane_bg: egui::Color32,
    pub pane_active_accent: egui::Color32,
    pub pane_inactive_accent: egui::Color32,

    // Scrollbar
    pub scrollbar_track: egui::Color32,
    pub scrollbar_thumb: egui::Color32,
    pub scrollbar_thumb_hover: egui::Color32,
    pub scrollbar_thumb_active: egui::Color32,

    // Row backgrounds
    pub row_selected: egui::Color32,
    pub row_marked: egui::Color32,
    pub row_disabled: egui::Color32,
    pub row_even: egui::Color32,
    pub row_odd: egui::Color32,
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
            panel_fill: egui::Color32::from_rgb(24, 26, 28),
            window_fill: egui::Color32::from_rgb(31, 33, 36),
            faint_bg: egui::Color32::from_rgb(37, 41, 42),
            selection_bg: egui::Color32::from_rgb(66, 96, 89),
            accent: egui::Color32::from_rgb(132, 184, 153),
            text: egui::Color32::WHITE,
            top_bottom_panel: egui::Color32::from_rgb(31, 35, 34),
            central_panel: egui::Color32::from_rgb(20, 22, 23),
            modal_fill: egui::Color32::from_rgb(38, 42, 43),
            modal_border: egui::Color32::from_rgb(132, 184, 153),
            input_help_fill: egui::Color32::from_rgb(34, 38, 39),
            text_input_fill: egui::Color32::from_rgb(35, 39, 40),
            text_input_field_fill: egui::Color32::from_rgb(24, 27, 28),
            text_input_field_border: egui::Color32::from_rgb(80, 88, 90),
            focused_fill: egui::Color32::from_rgb(73, 104, 94),
            focused_stroke: egui::Color32::WHITE,
            pane_bg: egui::Color32::from_rgb(34, 37, 39),
            pane_active_accent: egui::Color32::from_rgb(132, 184, 153),
            pane_inactive_accent: egui::Color32::from_rgb(82, 88, 90),
            scrollbar_track: egui::Color32::from_rgb(44, 47, 50),
            scrollbar_thumb: egui::Color32::from_rgb(75, 82, 86),
            scrollbar_thumb_hover: egui::Color32::from_rgb(100, 110, 115),
            scrollbar_thumb_active: egui::Color32::from_rgb(120, 130, 135),
            row_selected: egui::Color32::from_rgb(59, 78, 73),
            row_marked: egui::Color32::from_rgb(58, 52, 35),
            row_disabled: egui::Color32::from_rgb(28, 30, 31),
            row_even: egui::Color32::from_rgb(38, 42, 43),
            row_odd: egui::Color32::from_rgb(32, 35, 36),
        }
    }

    fn light() -> Self {
        Self {
            panel_fill: egui::Color32::from_rgb(240, 242, 244),
            window_fill: egui::Color32::from_rgb(248, 249, 250),
            faint_bg: egui::Color32::from_rgb(228, 231, 234),
            selection_bg: egui::Color32::from_rgb(140, 190, 165),
            accent: egui::Color32::from_rgb(40, 120, 80),
            text: egui::Color32::BLACK,
            top_bottom_panel: egui::Color32::from_rgb(218, 223, 228),
            central_panel: egui::Color32::from_rgb(230, 234, 237),
            modal_fill: egui::Color32::from_rgb(248, 249, 250),
            modal_border: egui::Color32::from_rgb(40, 120, 80),
            input_help_fill: egui::Color32::from_rgb(243, 245, 247),
            text_input_fill: egui::Color32::from_rgb(248, 249, 250),
            text_input_field_fill: egui::Color32::from_rgb(255, 255, 255),
            text_input_field_border: egui::Color32::from_rgb(175, 182, 190),
            focused_fill: egui::Color32::from_rgb(140, 190, 165),
            focused_stroke: egui::Color32::BLACK,
            pane_bg: egui::Color32::from_rgb(238, 241, 244),
            pane_active_accent: egui::Color32::from_rgb(40, 120, 80),
            pane_inactive_accent: egui::Color32::from_rgb(148, 158, 165),
            scrollbar_track: egui::Color32::from_rgb(208, 213, 218),
            scrollbar_thumb: egui::Color32::from_rgb(172, 180, 188),
            scrollbar_thumb_hover: egui::Color32::from_rgb(148, 158, 165),
            scrollbar_thumb_active: egui::Color32::from_rgb(118, 128, 136),
            row_selected: egui::Color32::from_rgb(175, 213, 196),
            row_marked: egui::Color32::from_rgb(243, 218, 148),
            row_disabled: egui::Color32::from_rgb(212, 216, 220),
            row_even: egui::Color32::from_rgb(243, 245, 248),
            row_odd: egui::Color32::from_rgb(232, 235, 239),
        }
    }
}
