pub mod button_images;
pub mod icons;
pub mod panes;
pub mod text_input;

use eframe::egui;

use crate::{
    action::CommanderAction,
    app::APP_TITLE,
    settings::{self, ControllerOverride},
    state::{AppState, DialogState, SettingsModalFocus, TransferConflictState},
    tasks::{TaskId, TaskStatus},
    theme::ThemeColors,
};

use button_images::{ButtonImages, btn};

pub fn show(ui: &mut egui::Ui, state: &AppState, images: &ButtonImages) -> Vec<CommanderAction> {
    let mut actions = Vec::new();
    let tc = ThemeColors::for_theme(state.settings.theme);

    egui::Panel::top("top_title")
        .exact_size(40.0)
        .frame(panel_frame(tc.top_bottom_panel))
        .show_inside(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(2.0);
                ui.heading(APP_TITLE);
            });
        });

    egui::Panel::bottom("bottom_status")
        .exact_size(30.0)
        .frame(panel_frame(tc.top_bottom_panel))
        .show_inside(ui, |ui| footer(ui, state));

    egui::CentralPanel::default()
        .frame(panel_frame(tc.central_panel))
        .show_inside(ui, |ui| panes::show(ui, state, &mut actions));

    input_help_overlay(ui.ctx(), state, images, &tc);
    text_input::show(ui.ctx(), state, &mut actions, images);
    modal_overlay(ui.ctx(), state, &mut actions, images, &tc);

    actions
}

fn footer(ui: &mut egui::Ui, state: &AppState) {
    ui.label(&state.status);
}

fn modal_overlay(
    ctx: &egui::Context,
    state: &AppState,
    actions: &mut Vec<CommanderAction>,
    images: &ButtonImages,
    tc: &ThemeColors,
) {
    let dialog = match &state.dialog {
        DialogState::Message { .. }
        | DialogState::DeleteConfirm(_)
        | DialogState::ShortcutConflict(_)
        | DialogState::TransferConflict(_)
        | DialogState::CommandMenu(_)
        | DialogState::Settings(_)
        | DialogState::Favourites(_)
        | DialogState::Decompress(_)
        | DialogState::SortBy(_)
        | DialogState::OperationProgress(_) => &state.dialog,
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
                .fill(tc.modal_fill)
                .stroke(egui::Stroke::new(1.0, tc.modal_border))
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_min_width(480.0);
                    match dialog {
                        DialogState::Message { title, message } => {
                            ui.heading(title);
                            ui.label(message);
                        }
                        DialogState::DeleteConfirm(confirm) => {
                            let count = confirm.item_count();
                            let items_label = if count == 1 { "item" } else { "item(s)" };
                            if state.settings.use_recycle_bin {
                                ui.heading("Delete to Recycle Bin");
                                ui.label(format!(
                                    "Move {} {} to the Recycle Bin?",
                                    count, items_label
                                ));
                            } else {
                                ui.heading("Permanently Delete");
                                ui.label(format!("Permanently delete {} {}?", count, items_label));
                            }
                            let examples_label = if count == 1 { "File" } else { "Examples" };
                            ui.label(format!("{}: {}", examples_label, confirm.examples()));
                            ui.add_space(8.0);
                            for (index, label) in ["Delete", "Cancel"].iter().enumerate() {
                                let focused = index == confirm.focused_index;
                                let mut button = egui::Button::new(*label);
                                if focused {
                                    button = button
                                        .fill(tc.focused_fill)
                                        .stroke(egui::Stroke::new(2.0, tc.focused_stroke));
                                }
                                if ui.add_sized([ui.available_width(), 34.0], button).clicked() {
                                    if index == 0 {
                                        actions.push(CommanderAction::Confirm);
                                    } else {
                                        actions.push(CommanderAction::Cancel);
                                    }
                                }
                            }
                            ui.add_space(8.0);
                            gamepad_hint_move_select_cancel(ui, images);
                        }
                        DialogState::ShortcutConflict(confirm) => {
                            ui.heading("Overwrite Shortcut");
                            ui.label(format!(
                                "{} already exists.",
                                confirm.shortcut_path().display()
                            ));
                            ui.label(format!("Target: {}", confirm.target_path().display()));
                            ui.add_space(8.0);
                            for (index, label) in ["Overwrite", "Cancel"].iter().enumerate() {
                                let focused = index == confirm.focused_index;
                                let mut button = egui::Button::new(*label);
                                if focused {
                                    button = button
                                        .fill(tc.focused_fill)
                                        .stroke(egui::Stroke::new(2.0, tc.focused_stroke));
                                }
                                if ui.add_sized([ui.available_width(), 34.0], button).clicked() {
                                    if index == 0 {
                                        actions.push(CommanderAction::Confirm);
                                    } else {
                                        actions.push(CommanderAction::Cancel);
                                    }
                                }
                            }
                            ui.add_space(8.0);
                            gamepad_hint_move_select_cancel(ui, images);
                        }
                        DialogState::TransferConflict(conflict) => {
                            render_transfer_conflict(ui, conflict, actions, images, tc);
                        }
                        DialogState::CommandMenu(menu) => {
                            ui.heading("Command Menu");
                            ui.add_space(8.0);
                            for (index, entry) in menu.entries.iter().enumerate() {
                                let focused = index == menu.focused_index;
                                let mut button = egui::Button::new(entry.label);
                                if focused {
                                    button = button
                                        .fill(tc.focused_fill)
                                        .stroke(egui::Stroke::new(2.0, tc.focused_stroke));
                                }
                                if ui.add_sized([ui.available_width(), 34.0], button).clicked() {
                                    actions.push(entry.action.clone());
                                }
                            }
                            ui.add_space(8.0);
                            gamepad_hint_move_select_cancel(ui, images);
                        }
                        DialogState::Settings(settings_modal) => {
                            ui.heading("Settings");
                            ui.add_space(8.0);
                            settings_row(
                                ui,
                                settings_modal.focused_setting() == SettingsModalFocus::UiScale,
                                tc,
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
                                tc,
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
                                tc,
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
                            settings_row(
                                ui,
                                settings_modal.focused_setting()
                                    == SettingsModalFocus::ControllerOverride,
                                tc,
                                |ui| {
                                    ui.strong("Controller Buttons");
                                    ui.add_space(18.0);
                                    if ui.button("◀").clicked() {
                                        actions.push(CommanderAction::AdjustFocusedSettingLeft);
                                    }
                                    ui.label(state.settings.controller_override.label());
                                    if ui.button("▶").clicked() {
                                        actions.push(CommanderAction::AdjustFocusedSettingRight);
                                    }
                                    if state.settings.controller_override
                                        == ControllerOverride::Auto
                                    {
                                        ui.label("(auto-detect)");
                                    }
                                },
                            );
                            ui.add_space(10.0);
                            settings_row(
                                ui,
                                settings_modal.focused_setting() == SettingsModalFocus::Theme,
                                tc,
                                |ui| {
                                    ui.strong("Theme");
                                    ui.add_space(18.0);
                                    if ui.button("◀").clicked() {
                                        actions.push(CommanderAction::AdjustFocusedSettingLeft);
                                    }
                                    ui.label(state.settings.theme.label());
                                    if ui.button("▶").clicked() {
                                        actions.push(CommanderAction::AdjustFocusedSettingRight);
                                    }
                                },
                            );
                            ui.add_space(8.0);
                            {
                                let focused =
                                    settings_modal.focused_setting() == SettingsModalFocus::Quit;
                                let mut button = egui::Button::new("Quit App")
                                    .stroke(egui::Stroke::new(2.0, egui::Color32::TRANSPARENT));
                                if focused {
                                    button = button
                                        .fill(tc.focused_fill)
                                        .stroke(egui::Stroke::new(2.0, tc.focused_stroke));
                                }
                                if ui.add_sized([260.0, 34.0], button).clicked() {
                                    actions.push(CommanderAction::QuitDevelopment);
                                }
                            }
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                btn(ui, images.dpad_all);
                                ui.label("choose/change");
                                ui.label(" · ");
                                btn(ui, images.south);
                                ui.label("toggle");
                                ui.label(" · ");
                                btn(ui, images.east);
                                ui.label("close");
                            });
                        }
                        DialogState::Favourites(favourites) => {
                            ui.heading("Favourites");
                            ui.add_space(8.0);
                            if state.settings.favourites.is_empty() {
                                ui.label("No favourite folders saved.");
                            } else {
                                for (index, favourite) in
                                    state.settings.favourites.iter().enumerate()
                                {
                                    let focused = index == favourites.focused_index;
                                    let exists = std::path::Path::new(favourite).is_dir();
                                    let label = if exists {
                                        crate::fs::location_label(favourite).to_owned()
                                    } else {
                                        format!(
                                            "{} (missing)",
                                            crate::fs::location_label(favourite)
                                        )
                                    };
                                    let mut button = egui::Button::new(label);
                                    if focused {
                                        button = button
                                            .fill(tc.focused_fill)
                                            .stroke(egui::Stroke::new(2.0, tc.focused_stroke));
                                    }
                                    if ui.add_sized([ui.available_width(), 34.0], button).clicked()
                                    {
                                        actions.push(CommanderAction::FocusFavourite(index));
                                        actions.push(CommanderAction::OpenFocusedFavourite);
                                    }
                                }
                            }
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                btn(ui, images.dpad_all);
                                ui.label("move");
                                ui.label(" · ");
                                btn(ui, images.south);
                                ui.label("open");
                                ui.label(" · ");
                                btn(ui, images.north);
                                ui.label("add");
                                ui.label(" · ");
                                btn(ui, images.west);
                                ui.label("remove");
                                ui.label(" · ");
                                btn(ui, images.east);
                                ui.label("close");
                            });
                        }
                        DialogState::Decompress(decompress) => {
                            ui.heading("Decompress");
                            ui.label(&decompress.archive.display_name);
                            ui.add_space(8.0);
                            for (index, choice) in decompress.choices().iter().enumerate() {
                                let focused = index == decompress.focused_index;
                                let mut button =
                                    egui::Button::new(choice.label(&decompress.archive_stem));
                                if focused {
                                    button = button
                                        .fill(tc.focused_fill)
                                        .stroke(egui::Stroke::new(2.0, tc.focused_stroke));
                                }
                                if ui.add_sized([360.0, 34.0], button).clicked() {
                                    actions.push(CommanderAction::FocusDecompressOption(index));
                                    actions.push(CommanderAction::Confirm);
                                }
                            }
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                btn(ui, images.dpad_all);
                                ui.label("move");
                                ui.label(" · ");
                                btn(ui, images.south);
                                ui.label("select");
                                ui.label(" · ");
                                btn(ui, images.east);
                                ui.label("cancel");
                            });
                        }
                        DialogState::SortBy(sort_by) => {
                            ui.heading("Sort By");
                            ui.add_space(8.0);
                            for (index, sort_order) in sort_by.choices().iter().enumerate() {
                                let focused = index == sort_by.focused_index;
                                let mut button = egui::Button::new(sort_order.label());
                                if focused {
                                    button = button
                                        .fill(tc.focused_fill)
                                        .stroke(egui::Stroke::new(2.0, tc.focused_stroke));
                                }
                                if ui.add_sized([320.0, 34.0], button).clicked() {
                                    actions.push(CommanderAction::SelectSortOrder(*sort_order));
                                }
                            }
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                btn(ui, images.dpad_all);
                                ui.label("move");
                                ui.label(" · ");
                                btn(ui, images.south);
                                ui.label("select");
                                ui.label(" · ");
                                btn(ui, images.east);
                                ui.label("cancel");
                            });
                        }
                        DialogState::OperationProgress(task_id) => {
                            render_operation_progress(ui, state, *task_id, images, tc);
                        }
                        DialogState::None | DialogState::TextInput(_) => {}
                    }
                });
        });
}

fn render_operation_progress(
    ui: &mut egui::Ui,
    state: &AppState,
    task_id: TaskId,
    images: &ButtonImages,
    tc: &ThemeColors,
) {
    let Some(task) = state.task_manager.task_by_id(task_id) else {
        ui.heading("Operation");
        ui.label("Loading...");
        return;
    };

    let done = task.status.is_finished();

    // Title row: kind + status
    ui.horizontal(|ui| {
        ui.heading(task.kind.label());
        if done {
            let status_color = match task.status {
                TaskStatus::Succeeded => tc.accent,
                TaskStatus::Failed => egui::Color32::LIGHT_RED,
                TaskStatus::Cancelled => egui::Color32::from_rgb(220, 184, 110),
                _ => tc.text,
            };
            ui.colored_label(status_color, format!("— {}", task.status.label()));
        }
    });

    // Task title (source/destination description)
    ui.add(egui::Label::new(&task.title).truncate());
    ui.add_space(8.0);

    // Progress bar — always shown; uses helpers from TaskProgress
    ui.add(
        egui::ProgressBar::new(task.progress_fraction())
            .desired_width(448.0)
            .text(task.progress_label()),
    );

    // Detail line: current file or error or counts
    ui.add_space(4.0);
    let detail = task.detail_label();
    let detail_color = if task.status == TaskStatus::Failed {
        egui::Color32::LIGHT_RED
    } else {
        egui::Color32::GRAY
    };
    ui.add(egui::Label::new(egui::RichText::new(&detail).color(detail_color)).truncate());

    // Byte / item counts
    if task.total_bytes.is_some() || task.total_items.is_some() {
        let mut parts: Vec<String> = Vec::new();
        if let Some(total) = task.total_items {
            parts.push(format!("{} of {} items", task.items_done, total));
        }
        if let Some(total) = task.total_bytes {
            parts.push(format!(
                "{} / {} MB",
                task.bytes_done / 1_048_576,
                total / 1_048_576
            ));
        }
        if !parts.is_empty() {
            ui.label(parts.join("  ·  "));
        }
    }

    if done {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            btn(ui, images.south);
            ui.label("close");
        });
    } else if task.cancellable {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if task.cancellation_requested {
                ui.label("cancelling...");
            } else {
                btn(ui, images.east);
                ui.label("cancel");
            }
        });
    }
}

fn input_help_overlay(
    ctx: &egui::Context,
    state: &AppState,
    images: &ButtonImages,
    tc: &ThemeColors,
) {
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
                .fill(tc.input_help_fill)
                .stroke(egui::Stroke::new(1.0, tc.modal_border))
                .inner_margin(egui::Margin::same(18))
                .show(ui, |ui| {
                    ui.heading("Input Help");
                    ui.add_space(8.0);
                    ui.horizontal_top(|ui| {
                        mapping_grid_left(ui, images);
                        ui.add_space(24.0);
                        mapping_grid_right(ui, images);
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label("F1 /");
                        btn(ui, images.select);
                        ui.label("toggles this overlay  · Esc /");
                        btn(ui, images.east);
                        ui.label("closes it");
                    });
                });
        });
}

fn mapping_grid_left(ui: &mut egui::Ui, images: &ButtonImages) {
    egui::Grid::new("input_help_grid_left")
        .num_columns(3)
        .spacing([18.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Action");
            ui.strong("Keyboard");
            ui.strong("Gamepad");
            ui.end_row();

            mapping_row_img(ui, "Move selection", "Up / Down", |ui| {
                btn(ui, images.dpad_up);
                btn(ui, images.dpad_down);
            });
            mapping_row_img(ui, "Page up / down", "Page Up / Down", |ui| {
                btn(ui, images.lb);
                btn(ui, images.rb);
            });
            mapping_row(ui, "Jump to start / end", "Home / End", "—");
            mapping_row_img(ui, "Switch pane", "Tab", |ui| {
                btn(ui, images.dpad_left);
                btn(ui, images.dpad_right);
            });
            mapping_row(ui, "Parent folder", "Backspace", "—");
            mapping_row_img(ui, "Open / confirm", "Enter", |ui| btn(ui, images.south));
            mapping_row_img(ui, "Cancel / back", "Esc", |ui| btn(ui, images.east));
            mapping_row_img(ui, "Toggle mark", "Space", |ui| btn(ui, images.west));
            mapping_row_img(ui, "Rename", "F2", |ui| btn(ui, images.north));
            mapping_row_img(ui, "Copy to other pane", "F5", |ui| btn(ui, images.north));
            mapping_row_img(ui, "Move to other pane", "F6", |ui| btn(ui, images.north));
            mapping_row_img(ui, "Create folder", "F7", |ui| btn(ui, images.north));
            mapping_row_img(ui, "Create file", "Ctrl+N", |ui| btn(ui, images.north));
            mapping_row_img(ui, "Create text file", "Ctrl+T", |ui| btn(ui, images.north));
        });
}

fn mapping_grid_right(ui: &mut egui::Ui, images: &ButtonImages) {
    egui::Grid::new("input_help_grid_right")
        .num_columns(3)
        .spacing([18.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Action");
            ui.strong("Keyboard");
            ui.strong("Gamepad");
            ui.end_row();

            mapping_row_img(ui, "Delete", "F8 / Delete", |ui| btn(ui, images.north));
            mapping_row_img(ui, "Refresh", "Ctrl+R", |ui| btn(ui, images.north));
            mapping_row_img(ui, "Favourites", "F3 / Ctrl+F", |ui| btn(ui, images.l3));
            mapping_row_img(ui, "Command menu", "—", |ui| btn(ui, images.north));
            mapping_row_img(ui, "Settings", "—", |ui| btn(ui, images.start));
            mapping_row_img(ui, "Input help", "F1", |ui| btn(ui, images.select));
            mapping_row_img(ui, "OSK: navigate", "Arrow keys", |ui| {
                btn(ui, images.dpad_all)
            });
            mapping_row_img(ui, "OSK: press key", "Type normally", |ui| {
                btn(ui, images.south)
            });
            mapping_row_img(ui, "OSK: backspace", "Backspace", |ui| btn(ui, images.west));
            mapping_row_img(ui, "OSK: symbols layer", "—", |ui| btn(ui, images.north));
            mapping_row_img(ui, "OSK: cursor left / right", "← →", |ui| {
                btn(ui, images.lb);
                btn(ui, images.rb);
            });
            mapping_row_img(ui, "OSK: submit", "Enter", |ui| btn(ui, images.start));
            mapping_row_img(ui, "OSK: cancel", "Esc", |ui| btn(ui, images.east));
        });
}

fn settings_row(
    ui: &mut egui::Ui,
    focused: bool,
    tc: &ThemeColors,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let frame = if focused {
        egui::Frame::new()
            .fill(tc.focused_fill)
            .stroke(egui::Stroke::new(2.0, tc.focused_stroke))
            .inner_margin(egui::Margin::same(6))
    } else {
        egui::Frame::new()
            .stroke(egui::Stroke::new(2.0, egui::Color32::TRANSPARENT))
            .inner_margin(egui::Margin::same(6))
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

fn mapping_row_img(
    ui: &mut egui::Ui,
    action: &str,
    keyboard: &str,
    gamepad_ui: impl FnOnce(&mut egui::Ui),
) {
    ui.label(action);
    ui.label(keyboard);
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), gamepad_ui);
    ui.end_row();
}

fn gamepad_hint_move_select_cancel(ui: &mut egui::Ui, images: &ButtonImages) {
    ui.horizontal(|ui| {
        btn(ui, images.dpad_all);
        ui.label("move");
        ui.label(" · ");
        btn(ui, images.south);
        ui.label("select");
        ui.label(" · ");
        btn(ui, images.east);
        ui.label("cancel");
    });
}

fn panel_frame(fill: egui::Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::same(10))
}

fn render_transfer_conflict(
    ui: &mut egui::Ui,
    conflict: &TransferConflictState,
    actions: &mut Vec<CommanderAction>,
    images: &ButtonImages,
    tc: &ThemeColors,
) {
    let verb = match conflict.kind {
        crate::tasks::TaskKind::Copy => "Copy",
        crate::tasks::TaskKind::Move => "Move",
        _ => "Transfer",
    };
    ui.heading(format!("{verb}: Item Already Exists"));
    ui.label(format!(
        "{} item(s) already exist in the target folder:",
        conflict.conflicts.len()
    ));
    ui.label(conflict.conflict_examples());
    ui.add_space(8.0);
    ui.label("Overwrite the existing item(s) or cancel?");
    ui.add_space(8.0);
    for (index, label) in ["Overwrite", "Cancel"].iter().enumerate() {
        let focused = index == conflict.focused_index;
        let mut button = egui::Button::new(*label);
        if focused {
            button = button
                .fill(tc.focused_fill)
                .stroke(egui::Stroke::new(2.0, tc.focused_stroke));
        }
        if ui.add_sized([ui.available_width(), 34.0], button).clicked() {
            if index == 0 {
                actions.push(CommanderAction::Confirm);
            } else {
                actions.push(CommanderAction::Cancel);
            }
        }
    }
    ui.add_space(8.0);
    gamepad_hint_move_select_cancel(ui, images);
}
