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
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let indicator = if is_active { "ACTIVE" } else { "STANDBY" };
                    ui.label(egui::RichText::new(indicator).color(accent).strong());
                });
            });

            pane_load_state(ui, &pane.load_state);
            ui.add_space(6.0);
            header_row(ui);
            ui.separator();

            let viewport_height = ui.available_height().max(0.0);
            let row_pitch = ENTRY_ROW_HEIGHT + ui.spacing().item_spacing.y;
            let row_layout = RowLayout::new(ui);
            let scroll_offset = if pane.needs_scroll_to_selected {
                scroll_offset_to_make_row_visible(
                    pane.selected_index,
                    pane.entries.len(),
                    pane.scroll_offset,
                    viewport_height,
                    row_pitch,
                    ENTRY_ROW_HEIGHT,
                )
            } else {
                pane.scroll_offset
            };

            let output = egui::ScrollArea::vertical()
                .id_salt(match side {
                    PaneSide::Left => "left_pane_entries",
                    PaneSide::Right => "right_pane_entries",
                })
                .vertical_scroll_offset(scroll_offset)
                .auto_shrink([false, false])
                .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                .show_viewport(ui, |ui, viewport| {
                    show_visible_rows(ui, pane, is_active, viewport, row_pitch, row_layout);
                });

            let offset = output.state.offset.y.round() as i32;
            let row_response = ui.interact(
                output.inner_rect,
                ui.make_persistent_id(match side {
                    PaneSide::Left => "left_pane_entry_click_target",
                    PaneSide::Right => "right_pane_entry_click_target",
                }),
                egui::Sense::click(),
            );
            if row_response.clicked()
                && let Some(index) =
                    clicked_row_index(ui, &row_response, offset, pane.entries.len())
            {
                actions.push(CommanderAction::SelectEntry { pane: side, index });
            }

            if pane.needs_scroll_to_selected {
                actions.push(CommanderAction::PaneScrollToSelectedIssued { pane: side, offset });
            } else if offset != pane.scroll_offset.round() as i32 {
                actions.push(CommanderAction::UpdatePaneScrollOffset { pane: side, offset });
            }

            ui.separator();
            bottom_status_bar(ui, pane, &pane.current_location);
        });
}

fn show_visible_rows(
    ui: &mut egui::Ui,
    pane: &PaneState,
    is_active: bool,
    viewport: egui::Rect,
    row_pitch: f32,
    row_layout: RowLayout,
) {
    let spacing = ui.spacing().item_spacing;
    ui.set_height((row_pitch * pane.entries.len() as f32 - spacing.y).max(0.0));

    let mut min_row = (viewport.min.y / row_pitch).floor() as usize;
    let mut max_row = (viewport.max.y / row_pitch).ceil() as usize + 1;
    if max_row > pane.entries.len() {
        let visible_row_count = max_row.saturating_sub(min_row);
        max_row = pane.entries.len();
        min_row = pane.entries.len().saturating_sub(visible_row_count);
    }

    let y_min = ui.max_rect().top() + min_row as f32 * row_pitch;
    let y_max = ui.max_rect().top() + max_row as f32 * row_pitch;
    let rect = egui::Rect::from_x_y_ranges(ui.max_rect().x_range(), y_min..=y_max);

    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.skip_ahead_auto_ids(min_row);
        for index in min_row..max_row {
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

            paint_entry_row(ui, row_layout, entry, selected, marked, disabled, fill);
        }
    });
}

fn clicked_row_index(
    ui: &egui::Ui,
    response: &egui::Response,
    scroll_offset: i32,
    entry_count: usize,
) -> Option<usize> {
    let click_pos = response.interact_pointer_pos()?;
    let row_pitch = ENTRY_ROW_HEIGHT + ui.spacing().item_spacing.y;
    let y_in_content = click_pos.y - response.rect.top() + scroll_offset as f32;
    if y_in_content < 0.0 {
        return None;
    }

    let row_index = (y_in_content / row_pitch).floor() as usize;
    let y_in_row = y_in_content - row_index as f32 * row_pitch;
    if row_index < entry_count && y_in_row <= ENTRY_ROW_HEIGHT {
        Some(row_index)
    } else {
        None
    }
}

fn scroll_offset_to_make_row_visible(
    selected_index: usize,
    entry_count: usize,
    current_offset: f32,
    viewport_height: f32,
    row_pitch: f32,
    row_height: f32,
) -> f32 {
    if entry_count == 0 {
        return 0.0;
    }

    let selected_index = selected_index.min(entry_count - 1);
    let row_top = selected_index as f32 * row_pitch;
    let row_bottom = row_top + row_height;
    let viewport_bottom = current_offset + viewport_height;

    if row_top < current_offset {
        row_top
    } else if row_bottom > viewport_bottom {
        (row_bottom - viewport_height).max(0.0)
    } else {
        current_offset.max(0.0)
    }
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

fn paint_entry_row(
    ui: &mut egui::Ui,
    layout: RowLayout,
    entry: &FileEntry,
    selected: bool,
    marked: bool,
    disabled: bool,
    fill: egui::Color32,
) {
    let row_rect = egui::Rect::from_min_size(
        ui.cursor().min,
        egui::vec2(ui.available_width(), ENTRY_ROW_HEIGHT),
    );
    ui.painter()
        .rect_filled(row_rect, egui::CornerRadius::ZERO, fill);
    paint_row_contents(
        ui,
        layout,
        row_rect.shrink2(egui::vec2(8.0, 5.0)),
        entry,
        selected,
        marked,
        disabled,
    );
    ui.advance_cursor_after_rect(row_rect);
}

#[derive(Clone, Copy)]
struct RowLayout {
    name_width: f32,
    spacing: f32,
}

impl RowLayout {
    fn new(ui: &egui::Ui) -> Self {
        Self {
            name_width: name_column_width(ui),
            spacing: ui.spacing().item_spacing.x,
        }
    }
}

fn paint_row_contents(
    ui: &mut egui::Ui,
    layout: RowLayout,
    content_rect: egui::Rect,
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

    let painter = ui.painter();
    let font_id = egui::TextStyle::Body.resolve(ui.style());
    let text_y = content_rect.center().y;

    let mut x = content_rect.left();
    paint_cell_text(
        painter,
        egui::Rect::from_min_size(
            egui::pos2(x, text_y - ROW_HEIGHT / 2.0),
            egui::vec2(layout.name_width, ROW_HEIGHT),
        ),
        &name,
        font_id.clone(),
        text_color,
        egui::Align2::LEFT_CENTER,
    );
    x += layout.name_width + layout.spacing;

    paint_cell_text(
        painter,
        egui::Rect::from_min_size(
            egui::pos2(x, text_y - ROW_HEIGHT / 2.0),
            egui::vec2(TYPE_COLUMN_WIDTH, ROW_HEIGHT),
        ),
        entry.kind_label(),
        font_id.clone(),
        text_color,
        egui::Align2::CENTER_CENTER,
    );
    x += TYPE_COLUMN_WIDTH + layout.spacing;

    paint_cell_text(
        painter,
        egui::Rect::from_min_size(
            egui::pos2(x, text_y - ROW_HEIGHT / 2.0),
            egui::vec2(SIZE_COLUMN_WIDTH, ROW_HEIGHT),
        ),
        &entry.size_label(),
        font_id.clone(),
        text_color,
        egui::Align2::RIGHT_CENTER,
    );
    x += SIZE_COLUMN_WIDTH + layout.spacing;

    paint_modified_cell_text(
        painter,
        egui::Rect::from_min_size(
            egui::pos2(x, content_rect.top()),
            egui::vec2(MODIFIED_COLUMN_WIDTH, content_rect.height()),
        ),
        &entry.modified_label(),
        font_id,
        text_color,
    );
}

fn paint_modified_cell_text(
    painter: &egui::Painter,
    rect: egui::Rect,
    text: &str,
    font_id: egui::FontId,
    color: egui::Color32,
) {
    let Some((date, time)) = text.split_once(' ') else {
        paint_cell_text(
            painter,
            rect,
            text,
            font_id,
            color,
            egui::Align2::LEFT_CENTER,
        );
        return;
    };

    let line_gap = 1.0;
    let line_height = font_id.size;
    let first_line_y = rect.center().y - (line_height + line_gap) / 2.0;
    let second_line_y = rect.center().y + (line_height + line_gap) / 2.0;
    let clipped_painter = painter.with_clip_rect(rect);

    clipped_painter.text(
        egui::pos2(rect.left(), first_line_y),
        egui::Align2::LEFT_CENTER,
        date,
        font_id.clone(),
        color,
    );
    clipped_painter.text(
        egui::pos2(rect.left(), second_line_y),
        egui::Align2::LEFT_CENTER,
        time,
        font_id,
        color,
    );
}

fn paint_cell_text(
    painter: &egui::Painter,
    rect: egui::Rect,
    text: &str,
    font_id: egui::FontId,
    color: egui::Color32,
    align: egui::Align2,
) {
    let pos = match align {
        egui::Align2::LEFT_CENTER => rect.left_center(),
        egui::Align2::CENTER_CENTER => rect.center(),
        egui::Align2::RIGHT_CENTER => rect.right_center(),
        _ => rect.left_center(),
    };
    painter
        .with_clip_rect(rect)
        .text(pos, align, text, font_id, color);
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

fn bottom_status_bar(ui: &mut egui::Ui, pane: &PaneState, location: &str) {
    let pane_summary = pane.pane_summary();
    let summary_text = pane_summary.format();

    let mut status_parts = vec![summary_text];

    if let Some(free_space) = crate::fs::get_free_space(location) {
        let free_label = crate::state::FileEntry::size_value_label(free_space);
        status_parts.push(format!("Free: {}", free_label));
    }

    ui.horizontal(|ui| {
        ui.set_height(ROW_HEIGHT);
        for (i, part) in status_parts.iter().enumerate() {
            if i > 0 {
                ui.separator();
            }
            ui.label(part);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::scroll_offset_to_make_row_visible;

    #[test]
    fn selected_row_above_viewport_scrolls_up_to_row() {
        let offset = scroll_offset_to_make_row_visible(2, 20, 300.0, 120.0, 40.0, 38.0);

        assert_eq!(offset, 80.0);
    }

    #[test]
    fn selected_row_below_viewport_scrolls_down_just_enough() {
        let offset = scroll_offset_to_make_row_visible(8, 20, 0.0, 120.0, 40.0, 38.0);

        assert_eq!(offset, 238.0);
    }

    #[test]
    fn selected_row_inside_viewport_keeps_current_offset() {
        let offset = scroll_offset_to_make_row_visible(4, 20, 120.0, 120.0, 40.0, 38.0);

        assert_eq!(offset, 120.0);
    }

    #[test]
    fn selected_first_row_with_stale_offset_scrolls_to_top() {
        let offset = scroll_offset_to_make_row_visible(0, 20, 400.0, 120.0, 40.0, 38.0);

        assert_eq!(offset, 0.0);
    }
}
