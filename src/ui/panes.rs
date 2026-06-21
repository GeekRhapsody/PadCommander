use eframe::egui;

use crate::{
    action::CommanderAction,
    state::{AppState, FileEntry, PaneLoadState, PaneSide, PaneState},
    theme::ThemeColors,
    ui::icons,
};

const TYPE_COLUMN_WIDTH: f32 = 110.0;
const SIZE_COLUMN_WIDTH: f32 = 74.0;
const MODIFIED_COLUMN_WIDTH: f32 = 90.0;
const MIN_NAME_COLUMN_WIDTH: f32 = 120.0;
const ROW_HEIGHT: f32 = 24.0;
const ENTRY_ROW_HEIGHT: f32 = 38.0;

pub fn show(ui: &mut egui::Ui, state: &AppState, actions: &mut Vec<CommanderAction>) {
    let tc = ThemeColors::for_theme(state.settings.theme);
    ui.columns(2, |columns| {
        pane(
            &mut columns[0],
            PaneSide::Left,
            &state.left_pane,
            state.active_pane == PaneSide::Left,
            actions,
            &tc,
        );
        pane(
            &mut columns[1],
            PaneSide::Right,
            &state.right_pane,
            state.active_pane == PaneSide::Right,
            actions,
            &tc,
        );
    });
}

fn pane(
    ui: &mut egui::Ui,
    side: PaneSide,
    pane: &PaneState,
    is_active: bool,
    actions: &mut Vec<CommanderAction>,
    tc: &ThemeColors,
) {
    let accent = if is_active {
        tc.pane_active_accent
    } else {
        tc.pane_inactive_accent
    };

    egui::Frame::new()
        .fill(tc.pane_bg)
        .stroke(egui::Stroke::new(if is_active { 2.0 } else { 1.0 }, accent))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_min_height(ui.available_height());

            ui.horizontal(|ui| {
                let marked_text = format!("Marked: {}", pane.marked_summary().size_label());
                let font_id = egui::TextStyle::Body.resolve(ui.style());
                let white = egui::Color32::WHITE;
                // separator width + item spacing on each side
                let sep_w = 1.0 + ui.spacing().item_spacing.x * 2.0;
                let marked_w = ui.ctx().fonts_mut(|f| {
                    f.layout_no_wrap(marked_text.clone(), font_id.clone(), white).size().x
                });
                let path_budget = (ui.available_width() - marked_w - sep_w).max(0.0);
                let path = crate::fs::location_label(&pane.current_location);
                let path_w = ui.ctx().fonts_mut(|f| {
                    f.layout_no_wrap(path.to_string(), font_id.clone(), white).size().x
                });
                let display_path = if path_w <= path_budget {
                    path.to_string()
                } else {
                    // Trim chars from the left until it fits (including the '…' prefix)
                    let chars: Vec<char> = path.chars().collect();
                    let mut start = 1;
                    loop {
                        let candidate: String = std::iter::once('…').chain(chars[start..].iter().copied()).collect();
                        let w = ui.ctx().fonts_mut(|f| {
                            f.layout_no_wrap(candidate.clone(), font_id.clone(), white).size().x
                        });
                        if w <= path_budget || start >= chars.len() - 1 {
                            break candidate;
                        }
                        start += 1;
                    }
                };
                ui.label(display_path);
                ui.separator();
                ui.label(marked_text);
            });

            pane_load_state(ui, &pane.load_state);
            ui.add_space(6.0);
            header_row(ui);
            ui.separator();

            let bottom_reserve = ROW_HEIGHT + ui.spacing().item_spacing.y * 2.0 + 1.0;
            let viewport_height = (ui.available_height() - bottom_reserve).max(0.0);
            let row_pitch = ENTRY_ROW_HEIGHT + ui.spacing().item_spacing.y;
            let row_layout = RowLayout::new(ui);
            let total_content_height =
                (row_pitch * pane.entries.len() as f32 - ui.spacing().item_spacing.y).max(0.0);
            let max_scroll = (total_content_height - viewport_height).max(0.0);

            // Start from state; apply scroll-to-selected if needed.
            let mut scroll_offset = if pane.needs_scroll_to_selected {
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

            // Geometry: content area + scrollbar side by side.
            const SCROLLBAR_WIDTH: f32 = 8.0;
            const SCROLLBAR_GAP: f32 = 4.0;
            let area_rect = egui::Rect::from_min_size(
                ui.cursor().min,
                egui::vec2(ui.available_width(), viewport_height),
            );
            let needs_scrollbar = max_scroll > 0.0;
            let content_width = if needs_scrollbar {
                (area_rect.width() - SCROLLBAR_WIDTH - SCROLLBAR_GAP).max(0.0)
            } else {
                area_rect.width()
            };
            let viewport_rect =
                egui::Rect::from_min_size(area_rect.min, egui::vec2(content_width, viewport_height));
            let scrollbar_rect = egui::Rect::from_min_size(
                area_rect.min + egui::vec2(content_width + SCROLLBAR_GAP, 0.0),
                egui::vec2(SCROLLBAR_WIDTH, viewport_height),
            );

            // Reserve the full area in the parent layout so the cursor advances.
            ui.allocate_rect(area_rect, egui::Sense::hover());

            // Mouse wheel scrolling when pointer is anywhere over the pane area.
            if ui.rect_contains_pointer(area_rect) {
                scroll_offset -= ui.input(|i| i.smooth_scroll_delta.y);
            }

            // Scrollbar thumb drag.
            if needs_scrollbar {
                let thumb_frac = (viewport_height / total_content_height).min(1.0);
                let thumb_height = (thumb_frac * viewport_height).clamp(20.0, viewport_height);
                let track_range = (viewport_height - thumb_height).max(1.0);
                let thumb_top =
                    (scroll_offset / max_scroll * track_range).clamp(0.0, track_range);
                let thumb_rect = egui::Rect::from_min_size(
                    scrollbar_rect.min + egui::vec2(0.0, thumb_top),
                    egui::vec2(SCROLLBAR_WIDTH, thumb_height),
                );

                let thumb_id = ui.id().with(match side {
                    PaneSide::Left => "left_scrollbar_thumb",
                    PaneSide::Right => "right_scrollbar_thumb",
                });
                let thumb_response =
                    ui.interact(thumb_rect, thumb_id, egui::Sense::click_and_drag());
                if thumb_response.dragged() {
                    scroll_offset += thumb_response.drag_delta().y * max_scroll / track_range;
                }

                // Clamp before computing final thumb position for painting.
                scroll_offset = scroll_offset.clamp(0.0, max_scroll);
                let thumb_top =
                    (scroll_offset / max_scroll * track_range).clamp(0.0, track_range);
                let thumb_rect = egui::Rect::from_min_size(
                    scrollbar_rect.min + egui::vec2(0.0, thumb_top),
                    egui::vec2(SCROLLBAR_WIDTH, thumb_height),
                );

                let painter = ui.painter();
                painter.rect_filled(
                    scrollbar_rect,
                    3.0,
                    tc.scrollbar_track,
                );
                let thumb_color = if thumb_response.is_pointer_button_down_on()
                    || thumb_response.dragged()
                {
                    tc.scrollbar_thumb_active
                } else if thumb_response.hovered() {
                    tc.scrollbar_thumb_hover
                } else {
                    tc.scrollbar_thumb
                };
                painter.rect_filled(thumb_rect, 3.0, thumb_color);
            }

            // Clamp final scroll offset.
            scroll_offset = scroll_offset.clamp(0.0, max_scroll);

            // Render visible rows into a child UI clipped to the viewport.
            {
                let content_origin = egui::pos2(viewport_rect.left(), viewport_rect.top() - scroll_offset);
                let content_max_rect = egui::Rect::from_min_size(
                    content_origin,
                    egui::vec2(content_width, total_content_height),
                );
                let clip_rect = viewport_rect.intersect(ui.clip_rect());
                let mut content_ui = ui.new_child(
                    egui::UiBuilder::new().max_rect(content_max_rect),
                );
                content_ui.set_clip_rect(clip_rect);
                let viewport_in_content = egui::Rect::from_min_size(
                    egui::pos2(0.0, scroll_offset),
                    egui::vec2(content_width, viewport_height),
                );
                show_visible_rows(
                    &mut content_ui,
                    pane,
                    is_active,
                    viewport_in_content,
                    row_pitch,
                    row_layout,
                    tc,
                );
            }

            // Click-to-select.
            let click_id = ui.id().with(match side {
                PaneSide::Left => "left_pane_entry_click_target",
                PaneSide::Right => "right_pane_entry_click_target",
            });
            let row_response = ui.interact(viewport_rect, click_id, egui::Sense::click());
            if row_response.clicked()
                && let Some(index) =
                    clicked_row_index(ui, &row_response, scroll_offset.round() as i32, pane.entries.len())
            {
                actions.push(CommanderAction::SelectEntry { pane: side, index });
            }

            actions.push(CommanderAction::SetPaneScrollOffset { pane: side, offset: scroll_offset.round() as i32 });

            ui.separator();
            bottom_status_bar(ui, pane);
        });
}

fn show_visible_rows(
    ui: &mut egui::Ui,
    pane: &PaneState,
    is_active: bool,
    viewport: egui::Rect,
    row_pitch: f32,
    row_layout: RowLayout,
    tc: &ThemeColors,
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
                tc.row_selected
            } else if marked {
                tc.row_marked
            } else if disabled {
                tc.row_disabled
            } else if index % 2 == 0 {
                tc.row_even
            } else {
                tc.row_odd
            };

            paint_entry_row(ui, row_layout, entry, selected, marked, disabled, fill, tc);
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
    let row_rect = egui::Rect::from_min_size(
        ui.cursor().min,
        egui::vec2(ui.available_width(), ROW_HEIGHT),
    );
    let content_rect = row_rect.shrink2(egui::vec2(8.0, 5.0));
    let layout = RowLayout::new(ui);
    let painter = ui.painter();
    let font_id = egui::TextStyle::Body.resolve(ui.style());
    let color = ui.visuals().strong_text_color();
    let text_y = content_rect.center().y;
    let mut x = content_rect.left();

    let cell = |x: f32, w: f32| {
        egui::Rect::from_min_size(
            egui::pos2(x, text_y - ROW_HEIGHT / 2.0),
            egui::vec2(w, ROW_HEIGHT),
        )
    };

    paint_cell_text(painter, cell(x, layout.name_width), "Name", font_id.clone(), color, egui::Align2::LEFT_CENTER);
    x += layout.name_width + layout.spacing;
    paint_cell_text(painter, cell(x, TYPE_COLUMN_WIDTH), "Type", font_id.clone(), color, egui::Align2::CENTER_CENTER);
    x += TYPE_COLUMN_WIDTH + layout.spacing;
    paint_cell_text(painter, cell(x, SIZE_COLUMN_WIDTH), "Size", font_id.clone(), color, egui::Align2::RIGHT_CENTER);
    x += SIZE_COLUMN_WIDTH + layout.spacing;
    paint_cell_text(painter, cell(x, MODIFIED_COLUMN_WIDTH), "Modified", font_id, color, egui::Align2::LEFT_CENTER);

    ui.advance_cursor_after_rect(row_rect);
}

fn paint_entry_row(
    ui: &mut egui::Ui,
    layout: RowLayout,
    entry: &FileEntry,
    selected: bool,
    marked: bool,
    disabled: bool,
    fill: egui::Color32,
    tc: &ThemeColors,
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
        tc,
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
    tc: &ThemeColors,
) {
    let name = if marked {
        format!("* {}", entry.display_name)
    } else {
        entry.display_name.clone()
    };
    let text_color = if disabled || selected {
        tc.text
    } else {
        ui.visuals().text_color()
    };

    let painter = ui.painter();
    let font_id = egui::TextStyle::Body.resolve(ui.style());
    let text_y = content_rect.center().y;

    let mut x = content_rect.left();

    // Icon
    let icon_gap = 5.0;
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(x + icons::ICON_SIZE / 2.0, text_y),
        egui::vec2(icons::ICON_SIZE, icons::ICON_SIZE),
    );
    icons::paint_entry_icon(painter, icon_rect, entry);
    let name_x = x + icons::ICON_SIZE + icon_gap;
    let name_width = (layout.name_width - icons::ICON_SIZE - icon_gap).max(0.0);

    paint_cell_text_elided(
        ui,
        painter,
        egui::Rect::from_min_size(
            egui::pos2(name_x, text_y - ROW_HEIGHT / 2.0),
            egui::vec2(name_width, ROW_HEIGHT),
        ),
        &name,
        font_id.clone(),
        text_color,
    );
    x += layout.name_width + layout.spacing;

    paint_cell_text(
        painter,
        egui::Rect::from_min_size(
            egui::pos2(x, text_y - ROW_HEIGHT / 2.0),
            egui::vec2(TYPE_COLUMN_WIDTH, ROW_HEIGHT),
        ),
        entry.type_display_label(),
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

fn paint_cell_text_elided(
    ui: &egui::Ui,
    painter: &egui::Painter,
    rect: egui::Rect,
    text: &str,
    font_id: egui::FontId,
    color: egui::Color32,
) {
    let galley = ui.ctx().fonts_mut(|f| {
        let mut job = egui::text::LayoutJob::single_section(
            text.to_string(),
            egui::TextFormat {
                font_id,
                color,
                ..Default::default()
            },
        );
        job.wrap = egui::text::TextWrapping {
            max_width: rect.width(),
            max_rows: 1,
            break_anywhere: true,
            overflow_character: Some('…'),
        };
        f.layout_job(job)
    });
    painter
        .with_clip_rect(rect)
        .galley(rect.left_center() - egui::vec2(0.0, galley.size().y / 2.0), galley, color);
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

fn bottom_status_bar(ui: &mut egui::Ui, pane: &PaneState) {
    let pane_summary = pane.pane_summary();
    let summary_text = pane_summary.format();

    let mut status_parts = vec![summary_text];

    if let Some(free_space) = pane.cached_free_space {
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
