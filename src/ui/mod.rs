pub mod action_bar;
pub mod panes;

use eframe::egui;

use crate::{action::Action, app::APP_TITLE, state::AppState};

pub fn show(ui: &mut egui::Ui, state: &AppState) -> Vec<Action> {
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
    let Some(modal) = &state.modal else {
        return;
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
                    ui.heading(&modal.title);
                    ui.label(&modal.message);
                });
        });
}

fn panel_frame(fill: egui::Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::same(10))
}
