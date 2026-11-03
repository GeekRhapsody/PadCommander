pub mod button_images;
pub mod icons;
pub mod panes;
pub mod text_input;

use eframe::egui;

use crate::{
    action::CommanderAction,
    app::APP_TITLE,
    settings::{self, ControllerOverride},
    state::{
        AppState, CommandMenuEntry, CommandMenuState, DialogState, PaneSide, SettingsModalFocus,
        TransferConflictState,
    },
    tasks::{TaskId, TaskProgress, TaskStatus},
    theme::{self, ThemeColors},
};

use button_images::{Btn, ButtonImages, btn, btn_sized};

const TOP_BAR_HEIGHT: f32 = 54.0;
const HINT_BAR_HEIGHT: f32 = 50.0;
const HINT_GLYPH_SIZE: f32 = 26.0;
const COMMAND_SHEET_WIDTH: f32 = 420.0;

pub fn show(ui: &mut egui::Ui, state: &AppState, images: &ButtonImages) -> Vec<CommanderAction> {
    let mut actions = Vec::new();
    let tc = ThemeColors::for_theme(state.settings.theme);

    egui::Panel::top("top_title")
        .exact_size(TOP_BAR_HEIGHT)
        .frame(bar_frame(tc.bar))
        .show_inside(ui, |ui| top_bar(ui, state, &tc));

    egui::Panel::bottom("bottom_status")
        .exact_size(HINT_BAR_HEIGHT)
        .frame(bar_frame(tc.bar))
        .show_inside(ui, |ui| hint_bar(ui, state, images, &tc));

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(tc.bg)
                .inner_margin(egui::Margin::symmetric(20, 16)),
        )
        .show_inside(ui, |ui| panes::show(ui, state, &mut actions));

    input_help_overlay(ui.ctx(), state, images, &tc);
    text_input::show(ui.ctx(), state, &mut actions, images);
    modal_overlay(ui.ctx(), state, &mut actions, images, &tc);

    actions
}

fn top_bar(ui: &mut egui::Ui, state: &AppState, tc: &ThemeColors) {
    ui.horizontal_centered(|ui| {
        let (logo_rect, _) = ui.allocate_exact_size(egui::vec2(26.0, 20.0), egui::Sense::hover());
        paint_logo(ui.painter(), logo_rect, tc);
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(APP_TITLE)
                .font(theme::bold(18.0))
                .color(tc.text),
        );

        // The progress dialog already shows the task, so only surface it here when it's hidden.
        let progress_dialog_open = matches!(state.dialog, DialogState::OperationProgress(_));
        if let Some(task) = state.task_manager.active_task()
            && !progress_dialog_open
        {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                task_chip(ui, task, tc);
            });
        }
    });
}

fn paint_logo(painter: &egui::Painter, rect: egui::Rect, tc: &ThemeColors) {
    let left = egui::Rect::from_min_size(
        rect.min + egui::vec2(1.0, 1.0),
        egui::vec2(rect.width() * 0.42, rect.height() - 2.0),
    );
    let right = egui::Rect::from_min_max(
        egui::pos2(rect.left() + rect.width() * 0.55, rect.top()),
        rect.max,
    );
    painter.rect_stroke(
        left,
        3,
        egui::Stroke::new(2.0_f32, tc.accent),
        egui::StrokeKind::Inside,
    );
    painter.rect_filled(right, 4, tc.accent);
}

fn task_chip(ui: &mut egui::Ui, task: &TaskProgress, tc: &ThemeColors) {
    egui::Frame::new()
        .fill(tc.chip)
        .corner_radius(255)
        .inner_margin(egui::Margin::symmetric(14, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let (spinner_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                paint_spinner(ui, spinner_rect, tc);
                ui.add(
                    egui::Label::new(egui::RichText::new(&task.title).color(tc.text)).truncate(),
                );
                let (bar_rect, _) =
                    ui.allocate_exact_size(egui::vec2(120.0, 6.0), egui::Sense::hover());
                paint_progress_bar(ui.painter(), bar_rect, task.progress_fraction(), tc);
                ui.label(
                    egui::RichText::new(task.progress_label())
                        .font(theme::mono(12.0))
                        .color(tc.muted),
                );
            });
        });
}

fn paint_spinner(ui: &egui::Ui, rect: egui::Rect, tc: &ThemeColors) {
    let center = rect.center();
    let radius = rect.width() / 2.0 - 1.5;
    ui.painter()
        .circle_stroke(center, radius, egui::Stroke::new(2.5_f32, tc.line));
    let start = ui.input(|i| i.time) as f32 * 5.0;
    let points = (0..=16)
        .map(|step| {
            let angle = start + step as f32 / 16.0 * std::f32::consts::FRAC_PI_2 * 1.4;
            center + radius * egui::vec2(angle.cos(), angle.sin())
        })
        .collect();
    ui.painter().add(egui::Shape::line(
        points,
        egui::Stroke::new(2.5_f32, tc.accent),
    ));
}

fn paint_progress_bar(painter: &egui::Painter, rect: egui::Rect, fraction: f32, tc: &ThemeColors) {
    let radius = (rect.height() / 2.0) as u8;
    painter.rect_filled(rect, radius, tc.line);
    let filled = egui::Rect::from_min_size(
        rect.min,
        egui::vec2(rect.width() * fraction.clamp(0.0, 1.0), rect.height()),
    );
    painter.rect_filled(filled, radius, tc.accent);
}

fn hint_bar(ui: &mut egui::Ui, state: &AppState, images: &ButtonImages, tc: &ThemeColors) {
    let hints: [(&[Btn], &str); 8] = [
        (&[images.dpad_all], "Move"),
        (&[images.south], "Open"),
        (&[images.east], "Back"),
        (&[images.west], "Mark"),
        (&[images.north], "Actions"),
        (&[images.lb, images.rb], "Page"),
        (&[images.start], "Settings"),
        (&[images.select], "Help"),
    ];

    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // Right-to-left layout, so add everything in reverse to read left-to-right.
        for (glyphs, label) in hints.iter().rev() {
            ui.label(egui::RichText::new(*label).color(tc.text));
            ui.add_space(-2.0);
            for glyph in glyphs.iter().rev() {
                btn_sized(ui, *glyph, HINT_GLYPH_SIZE);
            }
            ui.add_space(14.0);
        }

        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
            ui.painter()
                .circle_filled(dot_rect.center(), 4.0, tc.accent);
            ui.add_space(2.0);
            ui.add(egui::Label::new(egui::RichText::new(&state.status).color(tc.muted)).truncate());
        });
    });
}

fn render_command_sheet(
    ctx: &egui::Context,
    state: &AppState,
    menu: &CommandMenuState,
    actions: &mut Vec<CommanderAction>,
    images: &ButtonImages,
    tc: &ThemeColors,
) {
    let screen_rect = ctx.content_rect();
    paint_scrim(ctx, "command_sheet_scrim", tc);

    let width = COMMAND_SHEET_WIDTH.min(screen_rect.width());
    let (active, other, other_name) = match state.active_pane {
        PaneSide::Left => (&state.left_pane, &state.right_pane, "right"),
        PaneSide::Right => (&state.right_pane, &state.left_pane, "left"),
    };
    let marked = active.marked_summary();

    egui::Area::new(egui::Id::new("command_sheet"))
        .order(egui::Order::Tooltip)
        .fixed_pos(egui::pos2(screen_rect.right() - width, screen_rect.top()))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(tc.pane_active)
                .stroke(egui::Stroke::new(1.0_f32, tc.line))
                .inner_margin(egui::Margin {
                    left: 20,
                    right: 20,
                    top: 24,
                    bottom: 12,
                })
                .show(ui, |ui| {
                    ui.set_width(width - 40.0);
                    ui.spacing_mut().item_spacing.y = 4.0;

                    ui.label(
                        egui::RichText::new("Actions")
                            .font(theme::bold(22.0))
                            .color(tc.text),
                    );
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if marked.count > 0 {
                            egui::Frame::new()
                                .fill(tc.mark_fill)
                                .corner_radius(255)
                                .inner_margin(egui::Margin::symmetric(10, 3))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{} marked", marked.count))
                                            .font(theme::bold(13.0))
                                            .color(tc.mark),
                                    );
                                });
                        } else if let Some(entry) = active.selected_markable_entry() {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&entry.display_name).color(tc.text),
                                )
                                .truncate(),
                            );
                        }
                        ui.label(
                            egui::RichText::new(folder_name(&active.current_location))
                                .color(tc.muted),
                        );
                        // Painted, because Atkinson Hyperlegible has no arrow glyph.
                        let (arrow_rect, _) =
                            ui.allocate_exact_size(egui::vec2(18.0, 12.0), egui::Sense::hover());
                        paint_arrow_right(ui.painter(), arrow_rect, tc.accent);
                        ui.label(
                            egui::RichText::new(folder_name(&other.current_location))
                                .font(theme::bold(14.0))
                                .color(tc.text),
                        );
                    });
                    ui.add_space(8.0);

                    let mut current_group = None;
                    for (index, entry) in menu.entries.iter().enumerate() {
                        let group = command_group(&entry.action);
                        if current_group != Some(group) {
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(group)
                                    .font(theme::bold(10.5))
                                    .color(tc.faint),
                            );
                            current_group = Some(group);
                        }
                        let focused = index == menu.focused_index;
                        let note = command_note(&entry.action, state, marked.count);
                        if command_row(ui, entry, other_name, note, focused, tc).clicked() {
                            actions.push(entry.action.clone());
                        }
                    }

                    // Pin the button hints to the bottom of the sheet.
                    let used = ui.min_rect().height();
                    let target = screen_rect.height() - 36.0 - 48.0;
                    if used < target {
                        ui.add_space(target - used);
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.set_height(40.0);
                        for (glyph, label) in [
                            (images.dpad_all, "Move"),
                            (images.south, "Run"),
                            (images.east, "Close"),
                        ] {
                            btn_sized(ui, glyph, HINT_GLYPH_SIZE);
                            ui.label(label);
                            ui.add_space(12.0);
                        }
                    });
                });
        });
}

fn command_row(
    ui: &mut egui::Ui,
    entry: &CommandMenuEntry,
    other_pane: &str,
    note: Option<String>,
    focused: bool,
    tc: &ThemeColors,
) -> egui::Response {
    let height = if note.is_some() { 50.0 } else { 40.0 };
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    let destructive = entry.action == CommanderAction::Delete;
    let (fill, stroke) = if focused {
        (
            tc.focused_fill,
            egui::Stroke::new(1.5_f32, tc.focused_stroke),
        )
    } else if destructive {
        (tc.danger_fill, egui::Stroke::NONE)
    } else if response.hovered() {
        (tc.chip, egui::Stroke::NONE)
    } else {
        (egui::Color32::TRANSPARENT, egui::Stroke::NONE)
    };
    let painter = ui.painter();
    painter.rect(rect, 8, fill, stroke, egui::StrokeKind::Inside);

    let text_color = if destructive && !focused {
        tc.danger
    } else {
        tc.text
    };
    let icon_color = if focused {
        tc.focused_stroke
    } else {
        text_color
    };
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 12.0 + 9.0, rect.center().y),
        egui::vec2(18.0, 18.0),
    );
    paint_command_icon(painter, icon_rect, &entry.action, icon_color);

    let text_left = icon_rect.right() + 12.0;
    let label_font = if focused {
        theme::bold(15.0)
    } else {
        theme::regular(15.0)
    };
    let label = command_label(entry, other_pane);
    if let Some(note) = note {
        painter.text(
            egui::pos2(text_left, rect.center().y - 9.0),
            egui::Align2::LEFT_CENTER,
            label,
            label_font,
            text_color,
        );
        painter.text(
            egui::pos2(text_left, rect.center().y + 10.0),
            egui::Align2::LEFT_CENTER,
            note,
            theme::regular(12.0),
            tc.muted,
        );
    } else {
        painter.text(
            egui::pos2(text_left, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            label_font,
            text_color,
        );
    }

    if let Some(key) = command_shortcut(&entry.action) {
        let galley = painter.layout_no_wrap(key.to_owned(), theme::mono(11.5), tc.muted);
        let key_rect = egui::Rect::from_min_size(
            egui::pos2(
                rect.right() - 12.0 - galley.size().x - 12.0,
                rect.center().y - galley.size().y / 2.0 - 3.0,
            ),
            galley.size() + egui::vec2(12.0, 6.0),
        );
        painter.rect_stroke(
            key_rect,
            5,
            egui::Stroke::new(1.0_f32, tc.line),
            egui::StrokeKind::Inside,
        );
        painter.galley(key_rect.min + egui::vec2(6.0, 3.0), galley, tc.muted);
    }

    response
}

fn command_group(action: &CommanderAction) -> &'static str {
    match action {
        CommanderAction::CopyToOtherPane
        | CommanderAction::MoveToOtherPane
        | CommanderAction::CreateShortcutInOtherPane => "TRANSFER",
        CommanderAction::CompressToOtherPane | CommanderAction::DecompressToOtherPane => "ARCHIVE",
        CommanderAction::ConnectFtp | CommanderAction::DisconnectFtp => "REMOTE",
        CommanderAction::Delete => "REMOVE",
        _ => "ORGANIZE",
    }
}

fn command_label(entry: &CommandMenuEntry, other_pane: &str) -> String {
    match entry.action {
        CommanderAction::CopyToOtherPane => format!("Copy to {other_pane} pane"),
        CommanderAction::MoveToOtherPane => format!("Move to {other_pane} pane"),
        CommanderAction::CreateShortcutInOtherPane => {
            format!("Create shortcut in {other_pane} pane")
        }
        CommanderAction::CompressToOtherPane => format!("Compress to {other_pane} pane"),
        CommanderAction::DecompressToOtherPane => format!("Decompress to {other_pane} pane"),
        CommanderAction::CreateFolder => "New folder".to_owned(),
        CommanderAction::CreateTextFile => "New text file".to_owned(),
        CommanderAction::OpenSortBy => "Sort by…".to_owned(),
        CommanderAction::ConnectFtp => "Connect to server…".to_owned(),
        CommanderAction::DisconnectFtp => "Disconnect from server".to_owned(),
        _ => entry.label.to_owned(),
    }
}

fn command_note(action: &CommanderAction, state: &AppState, marked_count: usize) -> Option<String> {
    if *action != CommanderAction::Delete {
        return None;
    }
    let what = if marked_count > 1 { "them" } else { "it" };
    Some(if state.active_pane_is_on_server() {
        format!("Deletes {what} permanently from the server")
    } else if state.settings.use_recycle_bin {
        format!("Moves {what} to the Recycle Bin")
    } else {
        format!("Deletes {what} permanently")
    })
}

fn command_shortcut(action: &CommanderAction) -> Option<&'static str> {
    match action {
        CommanderAction::CopyToOtherPane => Some("F5"),
        CommanderAction::MoveToOtherPane => Some("F6"),
        CommanderAction::CreateFolder => Some("F7"),
        CommanderAction::Rename => Some("F2"),
        CommanderAction::Delete => Some("F8"),
        CommanderAction::CreateTextFile => Some("Ctrl+T"),
        CommanderAction::ConnectFtp => Some("Ctrl+F"),
        _ => None,
    }
}

fn paint_command_icon(
    painter: &egui::Painter,
    rect: egui::Rect,
    action: &CommanderAction,
    color: egui::Color32,
) {
    let stroke = egui::Stroke::new(1.6_f32, color);
    let p = |x: f32, y: f32| rect.min + egui::vec2(x, y) * (rect.width() / 18.0);
    let line = |points: Vec<egui::Pos2>| {
        painter.add(egui::Shape::line(points, stroke));
    };
    let frame = |min: egui::Pos2, max: egui::Pos2| {
        painter.rect_stroke(
            egui::Rect::from_min_max(min, max),
            2,
            stroke,
            egui::StrokeKind::Middle,
        );
    };

    match action {
        CommanderAction::CopyToOtherPane => {
            frame(p(6.0, 6.0), p(16.0, 17.0));
            line(vec![p(3.0, 12.0), p(3.0, 2.0), p(12.0, 2.0)]);
        }
        CommanderAction::MoveToOtherPane => {
            line(vec![p(2.0, 9.0), p(13.0, 9.0)]);
            line(vec![p(9.5, 5.0), p(13.5, 9.0), p(9.5, 13.0)]);
            line(vec![p(16.0, 3.0), p(16.0, 15.0)]);
        }
        CommanderAction::CreateShortcutInOtherPane => {
            line(vec![p(4.0, 14.0), p(14.0, 4.0)]);
            line(vec![p(7.0, 4.0), p(14.0, 4.0), p(14.0, 11.0)]);
        }
        CommanderAction::CompressToOtherPane | CommanderAction::DecompressToOtherPane => {
            frame(p(2.5, 2.5), p(15.5, 15.5));
            let (tip, base) = if *action == CommanderAction::CompressToOtherPane {
                (6.0, 12.0)
            } else {
                (12.0, 6.0)
            };
            let mid = (tip + base) / 2.0;
            line(vec![p(9.0, base), p(9.0, tip)]);
            line(vec![p(6.5, mid), p(9.0, tip), p(11.5, mid)]);
        }
        CommanderAction::CreateFolder => {
            line(vec![
                p(2.0, 15.0),
                p(2.0, 3.5),
                p(7.0, 3.5),
                p(8.5, 5.5),
                p(16.0, 5.5),
                p(16.0, 15.0),
                p(2.0, 15.0),
            ]);
            line(vec![p(9.0, 8.0), p(9.0, 13.0)]);
            line(vec![p(6.5, 10.5), p(11.5, 10.5)]);
        }
        CommanderAction::CreateTextFile => {
            line(vec![
                p(4.0, 2.0),
                p(11.0, 2.0),
                p(14.5, 5.5),
                p(14.5, 16.0),
                p(4.0, 16.0),
                p(4.0, 2.0),
            ]);
            line(vec![p(7.0, 9.0), p(12.0, 9.0)]);
            line(vec![p(7.0, 12.5), p(12.0, 12.5)]);
        }
        CommanderAction::Rename => {
            line(vec![
                p(3.0, 15.0),
                p(3.0, 12.0),
                p(12.0, 3.0),
                p(15.0, 6.0),
                p(6.0, 15.0),
                p(3.0, 15.0),
            ]);
        }
        CommanderAction::OpenSortBy => {
            line(vec![p(5.0, 3.0), p(5.0, 15.0)]);
            line(vec![p(2.5, 5.5), p(5.0, 3.0), p(7.5, 5.5)]);
            line(vec![p(13.0, 3.0), p(13.0, 15.0)]);
            line(vec![p(10.5, 12.5), p(13.0, 15.0), p(15.5, 12.5)]);
        }
        CommanderAction::ConnectFtp | CommanderAction::DisconnectFtp => {
            painter.circle_stroke(p(9.0, 9.0), rect.width() * 7.0 / 18.0, stroke);
            line(vec![p(2.0, 9.0), p(16.0, 9.0)]);
            line(vec![p(9.0, 2.0), p(6.5, 9.0), p(9.0, 16.0)]);
            line(vec![p(9.0, 2.0), p(11.5, 9.0), p(9.0, 16.0)]);
        }
        CommanderAction::Delete => {
            line(vec![p(2.5, 5.0), p(15.5, 5.0)]);
            line(vec![p(6.5, 5.0), p(6.5, 2.5), p(11.5, 2.5), p(11.5, 5.0)]);
            line(vec![p(4.5, 5.0), p(5.5, 16.0), p(12.5, 16.0), p(13.5, 5.0)]);
        }
        _ => {
            painter.circle_stroke(rect.center(), rect.width() / 3.0, stroke);
        }
    }
}

fn paint_arrow_right(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.6_f32, color);
    let y = rect.center().y;
    painter.line_segment(
        [
            egui::pos2(rect.left() + 1.0, y),
            egui::pos2(rect.right() - 1.0, y),
        ],
        stroke,
    );
    painter.add(egui::Shape::line(
        vec![
            egui::pos2(rect.right() - 6.0, y - 4.5),
            egui::pos2(rect.right() - 1.0, y),
            egui::pos2(rect.right() - 6.0, y + 4.5),
        ],
        stroke,
    ));
}

fn folder_name(location: &str) -> &str {
    let label = crate::fs::location_label(location);
    label
        .trim_end_matches(['\\', '/'])
        .rsplit(['\\', '/'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(label)
}

fn paint_scrim(ctx: &egui::Context, id: &str, tc: &ThemeColors) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new(id),
    ));
    painter.rect_filled(ctx.content_rect(), egui::CornerRadius::ZERO, tc.scrim);
}

fn modal_frame(tc: &ThemeColors) -> egui::Frame {
    egui::Frame::new()
        .fill(tc.pane_active)
        .stroke(egui::Stroke::new(1.0_f32, tc.line))
        .corner_radius(16)
        .inner_margin(egui::Margin::same(24))
}

fn modal_overlay(
    ctx: &egui::Context,
    state: &AppState,
    actions: &mut Vec<CommanderAction>,
    images: &ButtonImages,
    tc: &ThemeColors,
) {
    if let DialogState::CommandMenu(menu) = &state.dialog {
        render_command_sheet(ctx, state, menu, actions, images, tc);
        return;
    }

    let dialog = match &state.dialog {
        DialogState::Message { .. }
        | DialogState::DeleteConfirm(_)
        | DialogState::ShortcutConflict(_)
        | DialogState::TransferConflict(_)
        | DialogState::Settings(_)
        | DialogState::Favourites(_)
        | DialogState::FtpServers(_)
        | DialogState::SavePassword(_)
        | DialogState::Decompress(_)
        | DialogState::SortBy(_)
        | DialogState::OperationProgress(_) => &state.dialog,
        DialogState::None | DialogState::TextInput(_) | DialogState::CommandMenu(_) => return,
    };

    paint_scrim(ctx, "modal_overlay", tc);

    egui::Area::new(egui::Id::new("modal_overlay_area"))
        .order(egui::Order::Tooltip)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            modal_frame(tc).show(ui, |ui| {
                ui.set_min_width(480.0);
                match dialog {
                    DialogState::Message { title, message } => {
                        ui.heading(title);
                        ui.label(message);
                    }
                    DialogState::DeleteConfirm(confirm) => {
                        let count = confirm.item_count();
                        let items_label = if count == 1 { "item" } else { "item(s)" };
                        let on_server = confirm
                            .items
                            .iter()
                            .any(|entry| crate::ftp::is_ftp_location(&entry.virtual_path));
                        if on_server {
                            ui.heading("Delete from Server");
                            ui.label(format!(
                                "Permanently delete {} {} from the server?",
                                count, items_label
                            ));
                        } else if state.settings.use_recycle_bin {
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
                                    .stroke(egui::Stroke::new(1.5_f32, tc.focused_stroke));
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
                                    .stroke(egui::Stroke::new(1.5_f32, tc.focused_stroke));
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
                            settings_modal.focused_setting() == SettingsModalFocus::UseRecycleBin,
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
                            settings_modal.focused_setting() == SettingsModalFocus::ShowHiddenFiles,
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
                                if state.settings.controller_override == ControllerOverride::Auto {
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
                                .stroke(egui::Stroke::new(2.0_f32, egui::Color32::TRANSPARENT));
                            if focused {
                                button = button
                                    .fill(tc.focused_fill)
                                    .stroke(egui::Stroke::new(1.5_f32, tc.focused_stroke));
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
                            for (index, favourite) in state.settings.favourites.iter().enumerate() {
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
                                        .fill(tc.focused_fill)
                                        .stroke(egui::Stroke::new(1.5_f32, tc.focused_stroke));
                                }
                                if ui.add_sized([ui.available_width(), 34.0], button).clicked() {
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
                    DialogState::FtpServers(servers) => {
                        ui.heading("Servers");
                        ui.label(egui::RichText::new("FTP and FTPS connections").color(tc.muted));
                        ui.add_space(8.0);
                        let rows = state
                            .settings
                            .ftp_servers
                            .iter()
                            .map(|server| {
                                if server.saved_password.is_some() {
                                    format!("{}  (password saved)", server.label())
                                } else {
                                    server.label()
                                }
                            })
                            .chain(["+ Add server".to_owned()]);
                        for (index, label) in rows.enumerate() {
                            let mut button = egui::Button::new(label);
                            if index == servers.focused_index {
                                button = button
                                    .fill(tc.focused_fill)
                                    .stroke(egui::Stroke::new(1.5_f32, tc.focused_stroke));
                            }
                            if ui.add_sized([ui.available_width(), 34.0], button).clicked() {
                                actions.push(CommanderAction::FocusFtpServer(index));
                                actions.push(CommanderAction::Confirm);
                            }
                        }
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            btn(ui, images.dpad_all);
                            ui.label("move");
                            ui.label(" · ");
                            btn(ui, images.south);
                            ui.label("connect");
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
                    DialogState::SavePassword(prompt) => {
                        ui.heading("Save Password?");
                        ui.label(format!(
                            "Remember the password for {} on this device?",
                            prompt.server_label
                        ));
                        ui.label(
                            egui::RichText::new(
                                "It is encrypted with a key stored next to your settings.",
                            )
                            .color(tc.muted),
                        );
                        ui.add_space(8.0);
                        for (index, label) in ["Save password", "Don't save"].iter().enumerate() {
                            let mut button = egui::Button::new(*label);
                            if index == prompt.focused_index {
                                button = button
                                    .fill(tc.focused_fill)
                                    .stroke(egui::Stroke::new(1.5_f32, tc.focused_stroke));
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
                                    .stroke(egui::Stroke::new(1.5_f32, tc.focused_stroke));
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
                                    .stroke(egui::Stroke::new(1.5_f32, tc.focused_stroke));
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
                    DialogState::None | DialogState::TextInput(_) | DialogState::CommandMenu(_) => {
                    }
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
                TaskStatus::Failed => tc.danger,
                TaskStatus::Cancelled => tc.mark,
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
        tc.danger
    } else {
        tc.muted
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

    paint_scrim(ctx, "input_help_scrim", tc);

    egui::Area::new(egui::Id::new("input_help_overlay"))
        .order(egui::Order::Tooltip)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            modal_frame(tc).show(ui, |ui| {
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
            mapping_row_img(ui, "Parent folder", "Backspace", |ui| btn(ui, images.east));
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
            mapping_row_img(ui, "Command menu", "F9", |ui| btn(ui, images.north));
            mapping_row_img(ui, "Favourites", "F3 / Ctrl+D", |ui| btn(ui, images.l3));
            mapping_row_img(ui, "Servers (FTP)", "Ctrl+F", |ui| btn(ui, images.r3));
            mapping_row_img(ui, "List: add / remove", "Insert / Delete", |ui| {
                btn(ui, images.north);
                btn(ui, images.west);
            });
            mapping_row_img(ui, "Settings", "F10", |ui| btn(ui, images.start));
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
            .stroke(egui::Stroke::new(1.5_f32, tc.focused_stroke))
            .corner_radius(8)
            .inner_margin(egui::Margin::same(8))
    } else {
        egui::Frame::new()
            .stroke(egui::Stroke::new(1.5_f32, egui::Color32::TRANSPARENT))
            .corner_radius(8)
            .inner_margin(egui::Margin::same(8))
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

fn bar_frame(fill: egui::Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(24, 0))
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
                .stroke(egui::Stroke::new(1.5_f32, tc.focused_stroke));
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
