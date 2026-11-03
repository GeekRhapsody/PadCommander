use eframe::egui;

use crate::{
    action::CommanderAction,
    state::{
        AppState, FileEntry, FileEntryKind, PaneLoadState, PaneSide, PaneSortOrder, PaneState,
    },
    theme::{self, ThemeColors},
    ui::icons,
};

const PANE_GAP: f32 = 16.0;
const PANE_RADIUS: u8 = 12;
const PANE_PADDING: f32 = 16.0;
const HEADER_HEIGHT: f32 = 50.0;
const COLUMN_HEADER_HEIGHT: f32 = 28.0;
const FOOTER_HEIGHT: f32 = 40.0;
const LIST_INSET: f32 = 8.0;
const ROW_PADDING: f32 = 12.0;
const ROW_RADIUS: u8 = 8;
const ENTRY_ROW_HEIGHT: f32 = 40.0;
const ROW_GAP: f32 = 3.0;
const ROW_PITCH: f32 = ENTRY_ROW_HEIGHT + ROW_GAP;
const COLUMN_GAP: f32 = 10.0;
const CHECK_COLUMN_WIDTH: f32 = 16.0;
const SIZE_COLUMN_WIDTH: f32 = 76.0;
const MODIFIED_COLUMN_WIDTH: f32 = 124.0;
const SCROLLBAR_WIDTH: f32 = 5.0;
const SCROLLBAR_GAP: f32 = 4.0;

const NAME_FONT_SIZE: f32 = 14.0;
const DATA_FONT_SIZE: f32 = 12.0;
const CAPS_FONT_SIZE: f32 = 10.5;

pub fn show(ui: &mut egui::Ui, state: &AppState, actions: &mut Vec<CommanderAction>) {
    let tc = ThemeColors::for_theme(state.settings.theme);
    ui.spacing_mut().item_spacing.x = PANE_GAP;
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
    let pane_rect = ui.available_rect_before_wrap();
    ui.allocate_rect(pane_rect, egui::Sense::hover());

    let (fill, stroke) = if is_active {
        (tc.pane_active, egui::Stroke::new(2.5, tc.accent))
    } else {
        (tc.pane, egui::Stroke::new(1.0, tc.line))
    };
    let painter = ui.painter_at(pane_rect);
    painter.rect(
        pane_rect,
        PANE_RADIUS,
        fill,
        stroke,
        egui::StrokeKind::Inside,
    );

    let header_rect =
        egui::Rect::from_min_size(pane_rect.min, egui::vec2(pane_rect.width(), HEADER_HEIGHT));
    let column_header_rect = egui::Rect::from_min_size(
        header_rect.left_bottom(),
        egui::vec2(pane_rect.width(), COLUMN_HEADER_HEIGHT),
    );
    let footer_rect = egui::Rect::from_min_max(
        egui::pos2(pane_rect.left(), pane_rect.bottom() - FOOTER_HEIGHT),
        pane_rect.max,
    );
    let list_rect = egui::Rect::from_min_max(
        egui::pos2(
            pane_rect.left() + LIST_INSET,
            column_header_rect.bottom() + 6.0,
        ),
        egui::pos2(pane_rect.right() - LIST_INSET, footer_rect.top() - 6.0),
    );

    paint_header(ui, &painter, header_rect, side, pane, is_active, tc);
    paint_column_header(&painter, column_header_rect, pane.sort_order, tc);
    show_entry_list(ui, list_rect, side, pane, is_active, actions, tc);
    paint_footer(&painter, footer_rect, pane, tc);
}

fn paint_header(
    ui: &egui::Ui,
    painter: &egui::Painter,
    rect: egui::Rect,
    side: PaneSide,
    pane: &PaneState,
    is_active: bool,
    tc: &ThemeColors,
) {
    let center_y = rect.center().y;
    let mut left = rect.left() + PANE_PADDING;
    let mut right = rect.right() - PANE_PADDING;

    let tag = match side {
        PaneSide::Left => "LEFT",
        PaneSide::Right => "RIGHT",
    };
    let (tag_fill, tag_text) = if is_active {
        (tc.accent, tc.on_accent)
    } else {
        (tc.chip, tc.muted)
    };
    left += paint_chip(
        painter,
        egui::pos2(left, center_y),
        tag,
        theme::bold(CAPS_FONT_SIZE),
        tag_text,
        tag_fill,
        egui::vec2(7.0, 4.0),
        5,
    ) + 10.0;

    let marked = pane.marked_summary();
    if marked.count > 0 {
        // Folders have no known size, so only show a total when some files contribute to it.
        let mut text = format!("{} marked", marked.count);
        if marked.total_size > 0 {
            text.push_str(" · ");
            text.push_str(&FileEntry::size_value_label(marked.total_size));
            if marked.has_unknown_size {
                text.push('+');
            }
        }
        let galley = painter.layout_no_wrap(text, theme::bold(12.5), tc.mark);
        let pill_width = galley.size().x + 16.0 + 6.0 + 18.0;
        let pill_rect = egui::Rect::from_min_size(
            egui::pos2(right - pill_width, center_y - 13.0),
            egui::vec2(pill_width, 26.0),
        );
        painter.rect_filled(pill_rect, 13, tc.mark_fill);
        let check_center = egui::pos2(pill_rect.left() + 6.0 + 8.0, center_y);
        paint_check_badge(painter, check_center, 8.0, tc);
        painter.galley(
            egui::pos2(check_center.x + 8.0 + 6.0, center_y - galley.size().y / 2.0),
            galley,
            tc.mark,
        );
        right = pill_rect.left() - 12.0;
    }

    let (drive, crumbs) = breadcrumb_parts(crate::fs::location_label(&pane.current_location));
    if let Some(drive) = drive {
        left += paint_chip(
            painter,
            egui::pos2(left, center_y),
            &drive,
            theme::mono(DATA_FONT_SIZE),
            tc.muted,
            tc.chip,
            egui::vec2(8.0, 4.0),
            6,
        ) + 10.0;
    }

    paint_breadcrumb(ui, painter, left, right, center_y, &crumbs, tc);
}

fn paint_breadcrumb(
    ui: &egui::Ui,
    painter: &egui::Painter,
    left: f32,
    right: f32,
    center_y: f32,
    crumbs: &[String],
    tc: &ThemeColors,
) {
    const SEPARATOR_WIDTH: f32 = 18.0;
    let budget = (right - left).max(0.0);
    if crumbs.is_empty() || budget <= 0.0 {
        return;
    }

    let last_index = crumbs.len() - 1;
    let galleys: Vec<_> = crumbs
        .iter()
        .enumerate()
        .map(|(index, crumb)| {
            if index == last_index {
                painter.layout_no_wrap(crumb.clone(), theme::bold(NAME_FONT_SIZE), tc.text)
            } else {
                painter.layout_no_wrap(crumb.clone(), theme::regular(NAME_FONT_SIZE), tc.muted)
            }
        })
        .collect();
    let ellipsis = painter.layout_no_wrap("…".to_owned(), theme::regular(NAME_FONT_SIZE), tc.muted);

    // Drop leading crumbs (replacing them with "…") until the rest fits.
    let mut first = 0;
    loop {
        let mut width: f32 = galleys[first..]
            .iter()
            .map(|galley| galley.size().x)
            .sum::<f32>()
            + SEPARATOR_WIDTH * (last_index - first) as f32;
        if first > 0 {
            width += ellipsis.size().x + SEPARATOR_WIDTH;
        }
        if width <= budget || first == last_index {
            break;
        }
        first += 1;
    }

    let clip = painter.with_clip_rect(egui::Rect::from_x_y_ranges(
        left..=right,
        (center_y - 20.0)..=(center_y + 20.0),
    ));
    let mut x = left;
    if first > 0 {
        clip.galley(
            egui::pos2(x, center_y - ellipsis.size().y / 2.0),
            ellipsis.clone(),
            tc.muted,
        );
        x += ellipsis.size().x;
        paint_chevron(
            &clip,
            egui::pos2(x + SEPARATOR_WIDTH / 2.0, center_y),
            tc.faint,
        );
        x += SEPARATOR_WIDTH;
    }
    for (index, galley) in galleys.into_iter().enumerate().skip(first) {
        if index == last_index {
            let remaining = (right - x).max(0.0);
            paint_text_elided(
                ui,
                &clip,
                egui::Rect::from_min_max(
                    egui::pos2(x, center_y - 12.0),
                    egui::pos2(x + remaining, center_y + 12.0),
                ),
                &crumbs[index],
                theme::bold(NAME_FONT_SIZE),
                tc.text,
            );
        } else {
            let width = galley.size().x;
            clip.galley(
                egui::pos2(x, center_y - galley.size().y / 2.0),
                galley,
                tc.muted,
            );
            x += width;
            paint_chevron(
                &clip,
                egui::pos2(x + SEPARATOR_WIDTH / 2.0, center_y),
                tc.faint,
            );
            x += SEPARATOR_WIDTH;
        }
    }
}

fn paint_column_header(
    painter: &egui::Painter,
    rect: egui::Rect,
    sort_order: PaneSortOrder,
    tc: &ThemeColors,
) {
    let content = rect.shrink2(egui::vec2(LIST_INSET + ROW_PADDING, 0.0));
    let columns = RowColumns::new(content);
    let center_y = rect.center().y;

    let (sorted_column, ascending) = match sort_order {
        PaneSortOrder::FilenameAsc => (SortColumn::Name, true),
        PaneSortOrder::FilenameDesc => (SortColumn::Name, false),
        PaneSortOrder::SizeAsc => (SortColumn::Size, true),
        PaneSortOrder::SizeDesc => (SortColumn::Size, false),
        PaneSortOrder::DateAsc => (SortColumn::Modified, true),
        PaneSortOrder::DateDesc => (SortColumn::Modified, false),
    };

    for (column, label, x, align) in [
        (
            SortColumn::Name,
            "NAME",
            columns.name_x,
            egui::Align2::LEFT_CENTER,
        ),
        (
            SortColumn::Size,
            "SIZE",
            columns.size_right,
            egui::Align2::RIGHT_CENTER,
        ),
        (
            SortColumn::Modified,
            "MODIFIED",
            columns.modified_x,
            egui::Align2::LEFT_CENTER,
        ),
    ] {
        let sorted = column == sorted_column;
        let color = if sorted { tc.accent } else { tc.faint };
        let text_rect = painter.text(
            egui::pos2(x, center_y),
            align,
            label,
            theme::bold(CAPS_FONT_SIZE),
            color,
        );
        if sorted {
            let arrow_x = if align == egui::Align2::RIGHT_CENTER {
                text_rect.left() - 8.0
            } else {
                text_rect.right() + 8.0
            };
            paint_sort_arrow(painter, egui::pos2(arrow_x, center_y), ascending, color);
        }
    }

    painter.hline(
        (rect.left() + LIST_INSET)..=(rect.right() - LIST_INSET),
        rect.bottom(),
        egui::Stroke::new(1.0, tc.line),
    );
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortColumn {
    Name,
    Size,
    Modified,
}

#[allow(clippy::too_many_arguments)]
fn show_entry_list(
    ui: &mut egui::Ui,
    area_rect: egui::Rect,
    side: PaneSide,
    pane: &PaneState,
    is_active: bool,
    actions: &mut Vec<CommanderAction>,
    tc: &ThemeColors,
) {
    let viewport_height = area_rect.height().max(0.0);
    let total_content_height = (ROW_PITCH * pane.entries.len() as f32 - ROW_GAP).max(0.0);
    let max_scroll = (total_content_height - viewport_height).max(0.0);

    // Start from state; apply scroll-to-selected if needed.
    let mut scroll_offset = if pane.needs_scroll_to_selected {
        scroll_offset_to_make_row_visible(
            pane.selected_index,
            pane.entries.len(),
            pane.scroll_offset,
            viewport_height,
            ROW_PITCH,
            ENTRY_ROW_HEIGHT,
        )
    } else {
        pane.scroll_offset
    };

    let needs_scrollbar = max_scroll > 0.0;
    let content_width = if needs_scrollbar {
        (area_rect.width() - SCROLLBAR_WIDTH - SCROLLBAR_GAP).max(0.0)
    } else {
        area_rect.width()
    };
    let viewport_rect =
        egui::Rect::from_min_size(area_rect.min, egui::vec2(content_width, viewport_height));
    let scrollbar_rect = egui::Rect::from_min_size(
        egui::pos2(area_rect.right() - SCROLLBAR_WIDTH, area_rect.top()),
        egui::vec2(SCROLLBAR_WIDTH, viewport_height),
    );

    // Mouse wheel scrolling when pointer is anywhere over the list.
    if ui.rect_contains_pointer(area_rect) {
        scroll_offset -= ui.input(|i| i.smooth_scroll_delta.y);
    }

    // Scrollbar thumb drag.
    if needs_scrollbar {
        let thumb_frac = (viewport_height / total_content_height).min(1.0);
        let thumb_height = (thumb_frac * viewport_height).clamp(24.0, viewport_height);
        let track_range = (viewport_height - thumb_height).max(1.0);
        let thumb_top = (scroll_offset / max_scroll * track_range).clamp(0.0, track_range);
        let thumb_rect = egui::Rect::from_min_size(
            scrollbar_rect.min + egui::vec2(0.0, thumb_top),
            egui::vec2(SCROLLBAR_WIDTH, thumb_height),
        );

        let thumb_id = ui.id().with(match side {
            PaneSide::Left => "left_scrollbar_thumb",
            PaneSide::Right => "right_scrollbar_thumb",
        });
        // Widen the hit area so the slim thumb stays easy to grab.
        let thumb_response = ui.interact(
            thumb_rect.expand2(egui::vec2(4.0, 0.0)),
            thumb_id,
            egui::Sense::click_and_drag(),
        );
        if thumb_response.dragged() {
            scroll_offset += thumb_response.drag_delta().y * max_scroll / track_range;
        }

        // Clamp before computing final thumb position for painting.
        scroll_offset = scroll_offset.clamp(0.0, max_scroll);
        let thumb_top = (scroll_offset / max_scroll * track_range).clamp(0.0, track_range);
        let thumb_rect = egui::Rect::from_min_size(
            scrollbar_rect.min + egui::vec2(0.0, thumb_top),
            egui::vec2(SCROLLBAR_WIDTH, thumb_height),
        );

        let painter = ui.painter();
        painter.rect_filled(scrollbar_rect, 3, tc.line);
        let thumb_color = if thumb_response.is_pointer_button_down_on() || thumb_response.dragged()
        {
            tc.text
        } else if thumb_response.hovered() {
            tc.muted
        } else {
            tc.faint
        };
        painter.rect_filled(thumb_rect, 3, thumb_color);
    }

    // Clamp final scroll offset.
    scroll_offset = scroll_offset.clamp(0.0, max_scroll);

    paint_visible_rows(ui, viewport_rect, scroll_offset, pane, is_active, tc);

    // Click-to-select.
    let click_id = ui.id().with(match side {
        PaneSide::Left => "left_pane_entry_click_target",
        PaneSide::Right => "right_pane_entry_click_target",
    });
    let row_response = ui.interact(viewport_rect, click_id, egui::Sense::click());
    if row_response.clicked()
        && let Some(index) = row_response.interact_pointer_pos().and_then(|pos| {
            clicked_row_index(
                pos.y - viewport_rect.top(),
                scroll_offset,
                pane.entries.len(),
            )
        })
    {
        actions.push(CommanderAction::SelectEntry { pane: side, index });
    }

    actions.push(CommanderAction::SetPaneScrollOffset {
        pane: side,
        offset: scroll_offset.round() as i32,
    });
}

fn paint_visible_rows(
    ui: &egui::Ui,
    viewport: egui::Rect,
    scroll_offset: f32,
    pane: &PaneState,
    is_active: bool,
    tc: &ThemeColors,
) {
    if pane.entries.is_empty() {
        return;
    }

    let painter = ui.painter_at(viewport);
    let first_row = (scroll_offset / ROW_PITCH).floor().max(0.0) as usize;
    let last_row = (((scroll_offset + viewport.height()) / ROW_PITCH).ceil() as usize + 1)
        .min(pane.entries.len());

    for index in first_row..last_row {
        let entry = &pane.entries[index];
        let top = viewport.top() + index as f32 * ROW_PITCH - scroll_offset;
        let row_rect = egui::Rect::from_min_size(
            egui::pos2(viewport.left(), top),
            egui::vec2(viewport.width(), ENTRY_ROW_HEIGHT),
        );
        let row = RowState {
            cursor: index == pane.selected_index,
            pane_active: is_active,
            marked: pane.marked_items.contains(&entry.virtual_path),
        };
        paint_entry_row(ui, &painter, row_rect, entry, row, tc);
    }
}

#[derive(Clone, Copy)]
struct RowState {
    cursor: bool,
    pane_active: bool,
    marked: bool,
}

fn paint_entry_row(
    ui: &egui::Ui,
    painter: &egui::Painter,
    row_rect: egui::Rect,
    entry: &FileEntry,
    row: RowState,
    tc: &ThemeColors,
) {
    let (fill, stroke) = if row.cursor && row.pane_active {
        (tc.focused_fill, egui::Stroke::new(1.5, tc.focused_stroke))
    } else if row.cursor {
        let fill = if row.marked {
            tc.mark_fill
        } else {
            egui::Color32::TRANSPARENT
        };
        (fill, egui::Stroke::new(1.5, tc.line))
    } else if row.marked {
        (tc.mark_fill, egui::Stroke::NONE)
    } else {
        (egui::Color32::TRANSPARENT, egui::Stroke::NONE)
    };
    painter.rect(row_rect, ROW_RADIUS, fill, stroke, egui::StrokeKind::Inside);

    let content = row_rect.shrink2(egui::vec2(ROW_PADDING, 0.0));
    let columns = RowColumns::new(content);
    let center_y = row_rect.center().y;

    if row.marked {
        paint_check_badge(
            painter,
            egui::pos2(content.left() + CHECK_COLUMN_WIDTH / 2.0, center_y),
            7.5,
            tc,
        );
    }

    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(columns.icon_x + icons::ICON_SIZE / 2.0, center_y),
        egui::vec2(icons::ICON_SIZE, icons::ICON_SIZE),
    );
    icons::paint_entry_icon(painter, icon_rect, entry);

    let is_parent = entry.kind == FileEntryKind::Parent;
    let name = if is_parent {
        "Parent folder"
    } else {
        entry.display_name.as_str()
    };
    let name_color = if is_parent { tc.muted } else { tc.text };
    let name_font = if row.cursor && row.pane_active {
        theme::bold(NAME_FONT_SIZE)
    } else {
        theme::regular(NAME_FONT_SIZE)
    };
    paint_text_elided(
        ui,
        painter,
        egui::Rect::from_min_max(
            egui::pos2(columns.name_x, row_rect.top()),
            egui::pos2(columns.name_right, row_rect.bottom()),
        ),
        name,
        name_font,
        name_color,
    );

    painter.text(
        egui::pos2(columns.size_right, center_y),
        egui::Align2::RIGHT_CENTER,
        entry.size_label(),
        theme::mono(DATA_FONT_SIZE),
        tc.muted,
    );
    painter.text(
        egui::pos2(columns.modified_x, center_y),
        egui::Align2::LEFT_CENTER,
        entry.modified_label(),
        theme::mono(DATA_FONT_SIZE),
        tc.muted,
    );
}

/// Horizontal positions of the row columns, shared by the column header and rows.
struct RowColumns {
    icon_x: f32,
    name_x: f32,
    name_right: f32,
    size_right: f32,
    modified_x: f32,
}

impl RowColumns {
    fn new(content: egui::Rect) -> Self {
        let icon_x = content.left() + CHECK_COLUMN_WIDTH + COLUMN_GAP;
        let name_x = icon_x + icons::ICON_SIZE + COLUMN_GAP;
        let modified_x = content.right() - MODIFIED_COLUMN_WIDTH;
        let size_right = modified_x - COLUMN_GAP * 2.0;
        let name_right = (size_right - SIZE_COLUMN_WIDTH - COLUMN_GAP).max(name_x);
        Self {
            icon_x,
            name_x,
            name_right,
            size_right,
            modified_x,
        }
    }
}

fn paint_footer(painter: &egui::Painter, rect: egui::Rect, pane: &PaneState, tc: &ThemeColors) {
    painter.hline(rect.x_range(), rect.top(), egui::Stroke::new(1.0, tc.line));
    let center_y = rect.center().y;
    let left = rect.left() + PANE_PADDING;
    let right = rect.right() - PANE_PADDING;

    let (summary, color) = match &pane.load_state {
        PaneLoadState::Ready => (pane_summary_text(pane), tc.muted),
        PaneLoadState::Loading => ("Loading…".to_owned(), tc.muted),
        PaneLoadState::Error(error) => (error.clone(), tc.danger),
    };

    let mut summary_right = right;
    if let Some(free_space) = pane.cached_free_space {
        let free_rect = painter.text(
            egui::pos2(right, center_y),
            egui::Align2::RIGHT_CENTER,
            format!("{} free", FileEntry::size_value_label(free_space)),
            theme::regular(12.5),
            tc.muted,
        );
        summary_right = free_rect.left() - 16.0;
    }

    painter
        .with_clip_rect(egui::Rect::from_x_y_ranges(
            left..=summary_right,
            rect.y_range(),
        ))
        .text(
            egui::pos2(left, center_y),
            egui::Align2::LEFT_CENTER,
            summary,
            theme::regular(12.5),
            color,
        );
}

fn pane_summary_text(pane: &PaneState) -> String {
    let summary = pane.pane_summary();
    let plural = |count: usize, one: &str, many: &str| {
        format!("{count} {}", if count == 1 { one } else { many })
    };
    let mut parts = vec![plural(summary.file_count, "file", "files")];
    if summary.folder_count > 0 {
        parts.push(plural(summary.folder_count, "folder", "folders"));
    }
    if summary.file_count > 0 {
        parts.push(FileEntry::size_value_label(summary.total_size));
    }
    parts.join(" · ")
}

/// Splits a location into an optional drive or host chip and the folder crumbs after it.
fn breadcrumb_parts(location: &str) -> (Option<String>, Vec<String>) {
    if let Some((scheme, rest)) = location.split_once("://") {
        let mut segments = rest.split('/').filter(|segment| !segment.is_empty());
        let host = segments.next().unwrap_or_default();
        return (
            Some(format!("{scheme}://{host}")),
            segments.map(str::to_owned).collect(),
        );
    }

    let mut segments: Vec<String> = location
        .split(['\\', '/'])
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect();
    let is_drive = segments
        .first()
        .is_some_and(|first| first.len() == 2 && first.ends_with(':'));
    if is_drive {
        let drive = segments.remove(0);
        (Some(drive), segments)
    } else {
        (None, segments)
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_chip(
    painter: &egui::Painter,
    left_center: egui::Pos2,
    text: &str,
    font: egui::FontId,
    text_color: egui::Color32,
    fill: egui::Color32,
    padding: egui::Vec2,
    radius: u8,
) -> f32 {
    let galley = painter.layout_no_wrap(text.to_owned(), font, text_color);
    let size = galley.size() + padding * 2.0;
    let rect = egui::Rect::from_min_size(
        egui::pos2(left_center.x, left_center.y - size.y / 2.0),
        size,
    );
    painter.rect_filled(rect, radius, fill);
    painter.galley(rect.min + padding, galley, text_color);
    size.x
}

fn paint_check_badge(painter: &egui::Painter, center: egui::Pos2, radius: f32, tc: &ThemeColors) {
    painter.circle_filled(center, radius, tc.mark);
    let s = radius / 8.0;
    painter.add(egui::Shape::line(
        vec![
            center + egui::vec2(-3.6 * s, 0.2 * s),
            center + egui::vec2(-s, 2.8 * s),
            center + egui::vec2(3.8 * s, -2.6 * s),
        ],
        egui::Stroke::new(1.8 * s, tc.mark_fill),
    ));
}

fn paint_chevron(painter: &egui::Painter, center: egui::Pos2, color: egui::Color32) {
    painter.add(egui::Shape::line(
        vec![
            center + egui::vec2(-2.0, -4.5),
            center + egui::vec2(2.5, 0.0),
            center + egui::vec2(-2.0, 4.5),
        ],
        egui::Stroke::new(1.6, color),
    ));
}

fn paint_sort_arrow(
    painter: &egui::Painter,
    center: egui::Pos2,
    ascending: bool,
    color: egui::Color32,
) {
    // The arrowhead sits at -4 * dir, so ascending (dir = 1) points up.
    let dir = if ascending { 1.0 } else { -1.0 };
    let stroke = egui::Stroke::new(1.6, color);
    painter.line_segment(
        [
            center + egui::vec2(0.0, -4.0 * dir),
            center + egui::vec2(0.0, 4.0 * dir),
        ],
        stroke,
    );
    painter.add(egui::Shape::line(
        vec![
            center + egui::vec2(-3.0, -dir),
            center + egui::vec2(0.0, -4.0 * dir),
            center + egui::vec2(3.0, -dir),
        ],
        stroke,
    ));
}

fn paint_text_elided(
    ui: &egui::Ui,
    painter: &egui::Painter,
    rect: egui::Rect,
    text: &str,
    font_id: egui::FontId,
    color: egui::Color32,
) {
    if rect.width() <= 0.0 {
        return;
    }
    let galley = ui.ctx().fonts_mut(|f| {
        let mut job = egui::text::LayoutJob::single_section(
            text.to_owned(),
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
    painter.with_clip_rect(rect).galley(
        rect.left_center() - egui::vec2(0.0, galley.size().y / 2.0),
        galley,
        color,
    );
}

fn clicked_row_index(y_in_viewport: f32, scroll_offset: f32, entry_count: usize) -> Option<usize> {
    let y_in_content = y_in_viewport + scroll_offset;
    if y_in_content < 0.0 {
        return None;
    }

    let row_index = (y_in_content / ROW_PITCH).floor() as usize;
    let y_in_row = y_in_content - row_index as f32 * ROW_PITCH;
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

#[cfg(test)]
mod tests {
    use super::{
        ENTRY_ROW_HEIGHT, ROW_PITCH, breadcrumb_parts, clicked_row_index,
        scroll_offset_to_make_row_visible,
    };

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

    #[test]
    fn click_maps_to_row_accounting_for_scroll() {
        assert_eq!(clicked_row_index(5.0, 0.0, 10), Some(0));
        assert_eq!(clicked_row_index(5.0, ROW_PITCH * 3.0, 10), Some(3));
        assert_eq!(clicked_row_index(ENTRY_ROW_HEIGHT + 1.0, 0.0, 10), None);
        assert_eq!(clicked_row_index(ROW_PITCH * 10.0 + 1.0, 0.0, 10), None);
    }

    #[test]
    fn breadcrumb_splits_drive_from_folders() {
        let (drive, crumbs) = breadcrumb_parts(r"C:\Users\claud\Videos");

        assert_eq!(drive.as_deref(), Some("C:"));
        assert_eq!(crumbs, vec!["Users", "claud", "Videos"]);
    }

    #[test]
    fn breadcrumb_for_drive_root_has_no_crumbs() {
        let (drive, crumbs) = breadcrumb_parts(r"D:\");

        assert_eq!(drive.as_deref(), Some("D:"));
        assert!(crumbs.is_empty());
    }

    #[test]
    fn breadcrumb_for_ftp_uses_host_chip() {
        let (drive, crumbs) = breadcrumb_parts("ftp://example.com/pub/files");

        assert_eq!(drive.as_deref(), Some("ftp://example.com"));
        assert_eq!(crumbs, vec!["pub", "files"]);
    }

    #[test]
    fn breadcrumb_for_virtual_location_is_single_crumb() {
        let (drive, crumbs) = breadcrumb_parts("This PC");

        assert_eq!(drive, None);
        assert_eq!(crumbs, vec!["This PC"]);
    }
}
