use eframe::egui::{self, scroll_area::ScrollBarVisibility};

use crate::{
    action::CommanderAction,
    state::{AppState, FileEntry, PaneLoadState, PaneSide, PaneState},
};

const TYPE_COLUMN_WIDTH: f32 = 82.0;
const SIZE_COLUMN_WIDTH: f32 = 74.0;
const MODIFIED_COLUMN_WIDTH: f32 = 90.0;
const MIN_NAME_COLUMN_WIDTH: f32 = 120.0;
const ROW_HEIGHT: f32 = 24.0;
const ENTRY_ROW_HEIGHT: f32 = 38.0;

pub fn show(ui: &mut egui::Ui, state: &AppState, actions: &mut Vec<CommanderAction>) {
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
    actions: &mut Vec<CommanderAction>,
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
                ui.label(crate::fs::location_label(&pane.current_location));
                ui.separator();
                ui.label(format!("Marked: {}", pane.marked_summary().size_label()));
                ui.separator();
                ui.label(format!("Scroll: {:.0}", pane.scroll_offset));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let indicator = if is_active { "ACTIVE" } else { "STANDBY" };
                    ui.label(egui::RichText::new(indicator).color(accent).strong());
                });
            });

            pane_load_state(ui, &pane.load_state);
            ui.add_space(6.0);
            header_row(ui);
            ui.separator();

            egui::ScrollArea::vertical()
                .id_salt(match side {
                    PaneSide::Left => "left_pane_entries",
                    PaneSide::Right => "right_pane_entries",
                })
                .auto_shrink([false, false])
                .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                .show_rows(ui, ENTRY_ROW_HEIGHT, pane.entries.len(), |ui, row_range| {
                    for index in row_range {
                        let entry = &pane.entries[index];
                        let selected = is_active && index == pane.selected_index;
                        let marked = pane.marked_items.contains(&entry.virtual_path);
                        let disabled = !entry.is_available_source();
                        let fill = if selected {
                            egui::Color32::from_rgb(59, 78, 73)
                        } else if marked {
                            egui::Color32::from_rgb(58, 52, 35)
                        } else if disabled {
                            egui::Color32::from_rgb(28, 30, 31)
                        } else if index % 2 == 0 {
                            egui::Color32::from_rgb(38, 42, 43)
                        } else {
                            egui::Color32::from_rgb(32, 35, 36)
                        };

                        let response = egui::Frame::new()
                            .fill(fill)
                            .inner_margin(egui::Margin::symmetric(8, 5))
                            .show(ui, |ui| row_contents(ui, entry, selected, marked, disabled))
                            .response
                            .interact(egui::Sense::click())
                            .on_hover_text(&entry.virtual_path);

                        if selected {
                            response.scroll_to_me(Some(egui::Align::Center));
                        }

                        if response.clicked() {
                            actions.push(CommanderAction::SelectEntry { pane: side, index });
                        }
                    }
                });
        });
}

fn pane_load_state(ui: &mut egui::Ui, load_state: &PaneLoadState) {
    match load_state {
        PaneLoadState::Ready => {}
        PaneLoadState::Loading => {
            ui.label(egui::RichText::new("Loading...").italics());
        }
        PaneLoadState::Error(error) => {
            ui.label(egui::RichText::new(error).color(egui::Color32::LIGHT_RED));
        }
    }
}

fn header_row(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.set_height(ROW_HEIGHT);
        let name_width = name_column_width(ui);

        left_aligned_label(ui, name_width, egui::RichText::new("Name").strong());
        left_aligned_label(ui, TYPE_COLUMN_WIDTH, egui::RichText::new("Type").strong());
        left_aligned_label(ui, SIZE_COLUMN_WIDTH, egui::RichText::new("Size").strong());
        left_aligned_label(
            ui,
            MODIFIED_COLUMN_WIDTH,
            egui::RichText::new("Modified").strong(),
        );
    });
}

fn row_contents(
    ui: &mut egui::Ui,
    entry: &FileEntry,
    selected: bool,
    marked: bool,
    disabled: bool,
) {
    let name = if marked {
        format!("* {}", entry.display_name)
    } else {
        format!("  {}", entry.display_name)
    };
    let text_color = if disabled {
        egui::Color32::from_rgb(130, 136, 137)
    } else if selected {
        egui::Color32::WHITE
    } else {
        ui.visuals().text_color()
    };

    ui.horizontal(|ui| {
        ui.set_height(28.0);
        let name_width = name_column_width(ui);

        left_aligned_label(ui, name_width, egui::RichText::new(name).color(text_color));
        ui.add_sized(
            [TYPE_COLUMN_WIDTH, ROW_HEIGHT],
            egui::Label::new(egui::RichText::new(entry.kind_label()).color(text_color)).truncate(),
        );
        ui.add_sized(
            [SIZE_COLUMN_WIDTH, ROW_HEIGHT],
            egui::Label::new(egui::RichText::new(entry.size_label()).color(text_color)).truncate(),
        );
        ui.add_sized(
            [MODIFIED_COLUMN_WIDTH, ROW_HEIGHT],
            egui::Label::new(egui::RichText::new(entry.modified_label()).color(text_color))
                .truncate(),
        );
    });
}

fn name_column_width(ui: &egui::Ui) -> f32 {
    let spacing = ui.spacing().item_spacing.x;
    let fixed_width = TYPE_COLUMN_WIDTH + SIZE_COLUMN_WIDTH + MODIFIED_COLUMN_WIDTH + spacing * 3.0;
    (ui.available_width() - fixed_width).max(MIN_NAME_COLUMN_WIDTH)
}

fn left_aligned_label(ui: &mut egui::Ui, width: f32, text: egui::RichText) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, ROW_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_width(width);
            ui.add(egui::Label::new(text).truncate());
        },
    );
}
