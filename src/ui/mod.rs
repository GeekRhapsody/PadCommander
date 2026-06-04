pub mod action_bar;
pub mod panes;

use eframe::egui;

use crate::{
    action::CommanderAction,
    app::APP_TITLE,
    state::{AppState, DialogState},
};

pub fn show(ui: &mut egui::Ui, state: &AppState) -> Vec<CommanderAction> {
    let mut actions = Vec::new();

    egui::Panel::top("top_status")
        .exact_size(64.0)
        .frame(panel_frame(egui::Color32::from_rgb(31, 35, 34)))
        .show_inside(ui, |ui| header(ui, state));

    egui::Panel::bottom("command_bar")
        .exact_size(72.0)
        .frame(panel_frame(egui::Color32::from_rgb(28, 31, 33)))
        .show_inside(ui, |ui| action_bar::show(ui, &mut actions));

    egui::CentralPanel::default()
        .frame(panel_frame(egui::Color32::from_rgb(20, 22, 23)))
        .show_inside(ui, |ui| panes::show(ui, state, &mut actions));

    input_help_overlay(ui.ctx(), state);
    modal_overlay(ui.ctx(), state);

    actions
}

fn header(ui: &mut egui::Ui, state: &AppState) {
    ui.vertical_centered(|ui| {
        ui.add_space(2.0);
        ui.heading(APP_TITLE);
        ui.horizontal(|ui| {
            ui.label(format!("Active pane: {}", state.active_pane_label()));
            ui.separator();
            ui.label(format!(
                "Selected: {}",
                state
                    .active_pane()
                    .selected_entry()
                    .map(|entry| entry.display_name.as_str())
                    .unwrap_or("none")
            ));
            ui.separator();
            ui.label(format!("UI scale: {:.1}", state.settings.ui_scale));
            ui.separator();
            ui.label(format!("VFS: {}", state.vfs_label()));
            ui.separator();
            ui.label(format!("Tasks: {}", state.task_summary()));
            ui.separator();
            ui.label(&state.status);
        });
    });
}

fn modal_overlay(ctx: &egui::Context, state: &AppState) {
    let (title, message) = match &state.dialog {
        DialogState::Message { title, message } => (title.as_str(), message.as_str()),
        DialogState::TextInput { title, value } => (title.as_str(), value.as_str()),
        DialogState::None => return,
    };

    let screen_rect = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("modal_overlay"),
    ));
    painter.rect_filled(
        screen_rect,
        egui::CornerRadius::ZERO,
        egui::Color32::from_black_alpha(180),
    );

    egui::Area::new(egui::Id::new("modal_overlay_area"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(38, 42, 43))
                .stroke(egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgb(132, 184, 153),
                ))
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_min_width(360.0);
                    ui.heading(title);
                    ui.label(message);
                });
        });
}

fn input_help_overlay(ctx: &egui::Context, state: &AppState) {
    if !state.show_input_help {
        return;
    }

    let screen_rect = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("input_help_scrim"),
    ));
    painter.rect_filled(
        screen_rect,
        egui::CornerRadius::ZERO,
        egui::Color32::from_black_alpha(150),
    );

    egui::Area::new(egui::Id::new("input_help_overlay"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(34, 38, 39))
                .stroke(egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgb(132, 184, 153),
                ))
                .inner_margin(egui::Margin::same(18))
                .show(ui, |ui| {
                    ui.set_min_width(560.0);
                    ui.heading("Input Help");
                    ui.add_space(8.0);
                    mapping_grid(ui);
                    ui.add_space(8.0);
                    ui.label("F1 / View toggles this overlay. Esc / B closes it.");
                });
        });
}

fn mapping_grid(ui: &mut egui::Ui) {
    egui::Grid::new("input_help_mapping_grid")
        .num_columns(3)
        .spacing([18.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Action");
            ui.strong("Keyboard");
            ui.strong("Gamepad");
            ui.end_row();

            mapping_row(ui, "Move selection", "Up / Down", "D-pad Up / Down");
            mapping_row(ui, "Switch pane", "Tab", "D-pad Left / Right, LB / RB");
            mapping_row(ui, "Open / confirm", "Enter", "A");
            mapping_row(ui, "Cancel / back", "Esc", "B");
            mapping_row(ui, "Parent folder", "Backspace", "View reserved for help");
            mapping_row(ui, "Copy", "F5", "future command menu");
            mapping_row(ui, "Move", "F6", "future command menu");
            mapping_row(ui, "Create folder", "F7", "future command menu");
            mapping_row(ui, "Delete", "F8 / Delete", "future command menu");
            mapping_row(ui, "Rename", "F2", "future command menu");
            mapping_row(ui, "Refresh", "Ctrl+R", "future command menu");
            mapping_row(ui, "Mark item", "future shortcut", "X");
            mapping_row(ui, "Favourites", "future shortcut", "Y");
            mapping_row(ui, "Settings", "future shortcut", "Start / Menu");
            mapping_row(ui, "Quit development", "Ctrl+Q", "none");
        });
}

fn mapping_row(ui: &mut egui::Ui, action: &str, keyboard: &str, gamepad: &str) {
    ui.label(action);
    ui.label(keyboard);
    ui.label(gamepad);
    ui.end_row();
}

fn panel_frame(fill: egui::Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::same(10))
}
