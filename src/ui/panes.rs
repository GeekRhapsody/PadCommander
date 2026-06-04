use eframe::egui;

use crate::{
    action::Action,
    state::{AppState, PaneSide, PaneState},
};

pub fn show(ui: &mut egui::Ui, state: &AppState, actions: &mut Vec<Action>) {
    ui.columns(2, |columns| {
        pane(
            &mut columns[0],
            PaneSide::Left,
            &state.left_pane,
            state.active_pane == PaneSide::Left,
            actions,
        );
        pane(
            &mut columns[1],
            PaneSide::Right,
            &state.right_pane,
            state.active_pane == PaneSide::Right,
            actions,
        );
    });
}

fn pane(
    ui: &mut egui::Ui,
    side: PaneSide,
    pane: &PaneState,
    is_active: bool,
    actions: &mut Vec<Action>,
) {
    let accent = if is_active {
        egui::Color32::from_rgb(132, 184, 153)
    } else {
        egui::Color32::from_rgb(82, 88, 90)
    };

    egui::Frame::new()
        .fill(egui::Color32::from_rgb(34, 37, 39))
        .stroke(egui::Stroke::new(if is_active { 2.0 } else { 1.0 }, accent))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_min_height(ui.available_height());

            ui.horizontal(|ui| {
                ui.strong(&pane.title);
                ui.separator();
                ui.label(&pane.location);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let indicator = if is_active { "ACTIVE" } else { "STANDBY" };
                    ui.label(egui::RichText::new(indicator).color(accent).strong());
                });
            });

            ui.add_space(6.0);
            header_row(ui);
            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (index, row) in pane.rows.iter().enumerate() {
                        let selected = is_active && index == pane.selected_row;
                        let fill = if selected {
                            egui::Color32::from_rgb(59, 78, 73)
                        } else if index % 2 == 0 {
                            egui::Color32::from_rgb(38, 42, 43)
                        } else {
                            egui::Color32::from_rgb(32, 35, 36)
                        };

                        let response = egui::Frame::new()
                            .fill(fill)
                            .inner_margin(egui::Margin::symmetric(8, 5))
                            .show(ui, |ui| row_contents(ui, row))
                            .response
                            .interact(egui::Sense::click());

                        if response.clicked() {
                            actions.push(Action::ActivatePane(side));
                        }
                    }
                });
        });
}

fn header_row(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.set_height(24.0);
        ui.strong("Name");
        ui.add_space(220.0);
        ui.strong("Type");
        ui.add_space(64.0);
        ui.strong("Size");
        ui.add_space(64.0);
        ui.strong("Modified");
    });
}

fn row_contents(ui: &mut egui::Ui, row: &crate::state::FileRow) {
    ui.horizontal(|ui| {
        ui.set_height(28.0);
        ui.add_sized([240.0, 24.0], egui::Label::new(&row.name).truncate());
        ui.add_sized([82.0, 24.0], egui::Label::new(&row.kind).truncate());
        ui.add_sized([74.0, 24.0], egui::Label::new(&row.size).truncate());
        ui.add_sized([90.0, 24.0], egui::Label::new(&row.modified).truncate());
    });
}
