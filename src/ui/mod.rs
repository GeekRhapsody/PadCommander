pub mod action_bar;
pub mod panes;
pub mod progress;
pub mod text_input;

use eframe::egui;

use crate::{
    action::CommanderAction,
    app::APP_TITLE,
    settings,
    state::{AppState, DialogState, SettingsModalFocus},
};

pub fn show(ui: &mut egui::Ui, state: &AppState) -> Vec<CommanderAction> {
    let mut actions = Vec::new();

    egui::Panel::top("top_status")
        .exact_size(64.0)
        .frame(panel_frame(egui::Color32::from_rgb(31, 35, 34)))
        .show_inside(ui, |ui| header(ui, state));

    egui::Panel::bottom("command_bar")
        .exact_size(112.0)
        .frame(panel_frame(egui::Color32::from_rgb(28, 31, 33)))
        .show_inside(ui, |ui| action_bar::show(ui, state, &mut actions));

    egui::Panel::bottom("task_progress")
        .exact_size(118.0)
        .frame(panel_frame(egui::Color32::from_rgb(25, 29, 30)))
        .show_inside(ui, |ui| progress::show(ui, state, &mut actions));

    egui::CentralPanel::default()
        .frame(panel_frame(egui::Color32::from_rgb(20, 22, 23)))
        .show_inside(ui, |ui| panes::show(ui, state, &mut actions));

    input_help_overlay(ui.ctx(), state);
    text_input::show(ui.ctx(), state, &mut actions);
    modal_overlay(ui.ctx(), state, &mut actions);

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

fn modal_overlay(ctx: &egui::Context, state: &AppState, actions: &mut Vec<CommanderAction>) {
    let dialog = match &state.dialog {
        DialogState::Message { .. }
        | DialogState::DeleteConfirm(_)
        | DialogState::ShortcutConflict(_)
        | DialogState::CommandMenu(_)
        | DialogState::Settings(_)
        | DialogState::Favourites(_)
        | DialogState::Decompress(_) => &state.dialog,
        DialogState::None | DialogState::TextInput(_) => return,
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
        .order(egui::Order::Tooltip)
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
                    match dialog {
                        DialogState::Message { title, message } => {
                            ui.heading(title);
                            ui.label(message);
                        }
                        DialogState::DeleteConfirm(confirm) => {
                            if state.settings.use_recycle_bin {
                                ui.heading("Delete to Recycle Bin");
                                ui.label(format!(
                                    "Move {} item(s) to the Recycle Bin?",
                                    confirm.item_count()
                                ));
                            } else {
                                ui.heading("Permanently Delete");
                                ui.label(format!(
                                    "Permanently delete {} item(s)?",
                                    confirm.item_count()
                                ));
                            }
                            ui.label(format!("Examples: {}", confirm.examples()));
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                if ui.button("Delete").clicked() {
                                    actions.push(CommanderAction::Confirm);
                                }
                                if ui.button("Cancel").clicked() {
                                    actions.push(CommanderAction::Cancel);
                                }
                                ui.label("A / Enter confirms. B / Esc cancels.");
                            });
                        }
                        DialogState::ShortcutConflict(confirm) => {
                            ui.heading("Overwrite Shortcut");
                            ui.label(format!(
                                "{} already exists.",
                                confirm.shortcut_path().display()
                            ));
                            ui.label(format!(
                                "Target: {}",
                                confirm.target_path().display()
                            ));
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                if ui.button("Overwrite").clicked() {
                                    actions.push(CommanderAction::Confirm);
                                }
                                if ui.button("Cancel").clicked() {
                                    actions.push(CommanderAction::Cancel);
                                }
                                ui.label("A / Enter overwrites. B / Esc cancels.");
                            });
                        }
                        DialogState::CommandMenu(menu) => {
                            ui.heading("Command Menu");
                            ui.add_space(8.0);
                            for (index, entry) in menu.entries.iter().enumerate() {
                                let focused = index == menu.focused_index;
                                let mut button = egui::Button::new(entry.label);
                                if focused {
                                    button = button
                                        .fill(egui::Color32::from_rgb(73, 104, 94))
                                        .stroke(egui::Stroke::new(2.0, egui::Color32::WHITE));
                                }
                                if ui.add_sized([260.0, 34.0], button).clicked() {
                                    actions.push(entry.action.clone());
                                }
                            }
                            ui.add_space(8.0);
                            ui.label("D-pad/arrow keys move. A / Enter selects. B / Esc cancels.");
                        }
                        DialogState::Settings(settings_modal) => {
                            ui.heading("Settings");
                            ui.add_space(8.0);
                            settings_row(
                                ui,
                                settings_modal.focused_setting() == SettingsModalFocus::UiScale,
                                |ui| {
                                    ui.strong("UI scale");
                                    ui.add_space(18.0);
                                    if ui.button("-").clicked() {
                                        actions.push(CommanderAction::DecreaseUiScale);
                                    }
                                    ui.label(format!("{:.1}", state.settings.ui_scale));
                                    if ui.button("+").clicked() {
                                        actions.push(CommanderAction::IncreaseUiScale);
                                    }
                                },
                            );
                            ui.add(
                                egui::ProgressBar::new(
                                    (state.settings.ui_scale - settings::MIN_UI_SCALE)
                                        / (settings::MAX_UI_SCALE - settings::MIN_UI_SCALE),
                                )
                                .desired_width(260.0),
                            );
                            ui.add_space(10.0);
                            settings_row(
                                ui,
                                settings_modal.focused_setting()
                                    == SettingsModalFocus::UseRecycleBin,
                                |ui| {
                                    ui.strong("Use Recycle Bin");
                                    ui.add_space(18.0);
                                    let mut use_recycle_bin = state.settings.use_recycle_bin;
                                    if ui.checkbox(&mut use_recycle_bin, "").clicked() {
                                        actions.push(CommanderAction::ToggleUseRecycleBin);
                                    }
                                    ui.label(if state.settings.use_recycle_bin {
                                        "Enabled"
                                    } else {
                                        "Disabled"
                                    });
                                },
                            );
                            ui.add_space(10.0);
                            settings_row(
                                ui,
                                settings_modal.focused_setting()
                                    == SettingsModalFocus::ShowHiddenFiles,
                                |ui| {
                                    ui.strong("Show Hidden Files");
                                    ui.add_space(18.0);
                                    let mut show_hidden_files = state.settings.show_hidden_files;
                                    if ui.checkbox(&mut show_hidden_files, "").clicked() {
                                        actions.push(CommanderAction::ToggleShowHiddenFiles);
                                    }
                                    ui.label(if state.settings.show_hidden_files {
                                        "Yes"
                                    } else {
                                        "No"
                                    });
                                },
                            );
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                if ui.button("Apply").clicked() {
                                    actions.push(CommanderAction::Confirm);
                                }
                                if ui.button("Close").clicked() {
                                    actions.push(CommanderAction::Cancel);
                                }
                            });
                            ui.label("D-pad/arrow keys choose and change settings. A / Enter applies. B / Esc closes.");
                        }
                        DialogState::Favourites(favourites) => {
                            ui.heading("Favourites");
                            ui.add_space(8.0);
                            if state.settings.favourites.is_empty() {
                                ui.label("No favourite folders saved.");
                            } else {
                                for (index, favourite) in state.settings.favourites.iter().enumerate()
                                {
                                    let focused = index == favourites.focused_index;
                                    let exists = std::path::Path::new(favourite).is_dir();
                                    let label = if exists {
                                        crate::fs::location_label(favourite).to_owned()
                                    } else {
                                        format!("{} (missing)", crate::fs::location_label(favourite))
                                    };
                                    let mut button = egui::Button::new(label);
                                    if focused {
                                        button = button
                                            .fill(egui::Color32::from_rgb(73, 104, 94))
                                            .stroke(egui::Stroke::new(2.0, egui::Color32::WHITE));
                                    }
                                    if ui.add_sized([420.0, 34.0], button).clicked() {
                                        actions.push(CommanderAction::FocusFavourite(index));
                                        actions.push(CommanderAction::OpenFocusedFavourite);
                                    }
                                }
                            }
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                if ui.button("Add Current").clicked() {
                                    actions.push(CommanderAction::AddFavourite);
                                }
                                if ui.button("Remove").clicked() {
                                    actions.push(CommanderAction::RemoveFavourite);
                                }
                                if ui.button("Open").clicked() {
                                    actions.push(CommanderAction::OpenFocusedFavourite);
                                }
                                if ui.button("Close").clicked() {
                                    actions.push(CommanderAction::Cancel);
                                }
                            });
                            ui.label("D-pad/arrow keys move. A / Enter opens. X / Space removes. B / Esc closes.");
                        }
                        DialogState::Decompress(decompress) => {
                            ui.heading("Decompress");
                            ui.label(&decompress.archive.display_name);
                            ui.add_space(8.0);
                            for (index, choice) in decompress.choices().iter().enumerate() {
                                let focused = index == decompress.focused_index;
                                let mut button = egui::Button::new(choice.label(&decompress.archive_stem));
                                if focused {
                                    button = button
                                        .fill(egui::Color32::from_rgb(73, 104, 94))
                                        .stroke(egui::Stroke::new(2.0, egui::Color32::WHITE));
                                }
                                if ui.add_sized([360.0, 34.0], button).clicked() {
                                    actions.push(CommanderAction::FocusDecompressOption(index));
                                    actions.push(CommanderAction::Confirm);
                                }
                            }
                            ui.add_space(8.0);
                            ui.label("D-pad/arrow keys move. A / Enter selects. B / Esc cancels.");
                        }
                        DialogState::None | DialogState::TextInput(_) => {}
                    }
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
        .order(egui::Order::Tooltip)
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
            mapping_row(ui, "Create file", "Ctrl+N", "Y command menu");
            mapping_row(ui, "Create text file", "Ctrl+T", "Y command menu");
            mapping_row(ui, "Delete", "F8 / Delete", "future command menu");
            mapping_row(ui, "Rename", "F2", "future command menu");
            mapping_row(ui, "Refresh", "Ctrl+R", "future command menu");
            mapping_row(ui, "Mark item", "future shortcut", "X");
            mapping_row(ui, "Command menu", "future shortcut", "Y");
            mapping_row(ui, "Favourites", "F3 / Ctrl+F", "L3");
            mapping_row(ui, "Settings", "Start / Menu", "Start / Menu");
            mapping_row(ui, "Start task demo", "button", "Y command menu");
            mapping_row(ui, "Cancel active task", "Esc / button", "B / command menu");
            mapping_row(ui, "Text modal focus", "Arrow keys", "D-pad");
            mapping_row(ui, "Text modal key", "Type / Enter", "A");
            mapping_row(ui, "Text modal edit", "Backspace / Esc", "X / B");
            mapping_row(ui, "Text modal layer", "on-screen button", "Y");
            mapping_row(ui, "Text modal confirm", "Enter", "Start");
            mapping_row(ui, "Quit development", "Ctrl+Q", "none");
        });
}

fn settings_row(ui: &mut egui::Ui, focused: bool, add_contents: impl FnOnce(&mut egui::Ui)) {
    let frame = if focused {
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(73, 104, 94))
            .stroke(egui::Stroke::new(2.0, egui::Color32::WHITE))
            .inner_margin(egui::Margin::same(6))
    } else {
        egui::Frame::new().inner_margin(egui::Margin::same(6))
    };

    frame.show(ui, |ui| {
        ui.horizontal(add_contents);
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
