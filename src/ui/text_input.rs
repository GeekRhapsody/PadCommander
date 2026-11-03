use eframe::egui;

use crate::{
    action::{CommanderAction, TextInputAction},
    state::{AppState, DialogState, KeyboardLayer, OnScreenKey, TextInputDialogState},
    theme::{self, ThemeColors},
    ui::button_images::{ButtonImages, btn_sized},
};

const KEY_HEIGHT: f32 = 50.0;
const KEY_GAP: f32 = 8.0;
const FIELD_FONT_SIZE: f32 = 22.0;

pub fn show(
    ctx: &egui::Context,
    state: &AppState,
    actions: &mut Vec<CommanderAction>,
    images: &ButtonImages,
) {
    let DialogState::TextInput(dialog) = &state.dialog else {
        return;
    };

    let tc = ThemeColors::for_theme(state.settings.theme);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("text_input_scrim"),
    ));
    painter.rect_filled(ctx.content_rect(), egui::CornerRadius::ZERO, tc.scrim);

    // Docked to the bottom edge like a console keyboard, so the panes stay visible above it.
    egui::Area::new(egui::Id::new("text_input_dialog"))
        .order(egui::Order::Tooltip)
        .anchor(egui::Align2::CENTER_BOTTOM, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(tc.pane_active)
                .stroke(egui::Stroke::new(1.0, tc.line))
                .corner_radius(egui::CornerRadius {
                    nw: 20,
                    ne: 20,
                    sw: 0,
                    se: 0,
                })
                .inner_margin(egui::Margin {
                    left: 32,
                    right: 32,
                    top: 24,
                    bottom: 8,
                })
                .show(ui, |ui| text_input_dialog(ui, dialog, actions, images, &tc));
        });
}

fn text_input_dialog(
    ui: &mut egui::Ui,
    dialog: &TextInputDialogState,
    actions: &mut Vec<CommanderAction>,
    images: &ButtonImages,
    tc: &ThemeColors,
) {
    ui.set_width(keyboard_width(dialog));
    ui.spacing_mut().item_spacing.y = 8.0;

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&dialog.title)
                .font(theme::bold(22.0))
                .color(tc.text),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            layer_indicator(ui, dialog.layer, tc);
        });
    });

    input_field(ui, dialog, tc);

    if let Some(error) = &dialog.validation_error {
        ui.label(egui::RichText::new(error).color(tc.danger));
    }

    ui.add_space(6.0);
    on_screen_keyboard(ui, dialog, actions, tc);
    ui.add_space(4.0);
    ui.separator();

    ui.horizontal(|ui| {
        ui.set_height(40.0);
        let hints: [(&[_], &str); 7] = [
            (&[images.dpad_all], "Move"),
            (&[images.south], "Type"),
            (&[images.east], "Cancel"),
            (&[images.west], "Backspace"),
            (&[images.north], "Symbols"),
            (&[images.lb, images.rb], "Cursor"),
            (&[images.start], "Done"),
        ];
        for (glyphs, label) in hints {
            for glyph in glyphs {
                btn_sized(ui, *glyph, 26.0);
            }
            ui.label(egui::RichText::new(label).color(tc.text));
            ui.add_space(12.0);
        }
    });
}

fn input_field(ui: &mut egui::Ui, dialog: &TextInputDialogState, tc: &ThemeColors) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 56.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect(
        rect,
        12,
        tc.field,
        egui::Stroke::new(2.0, tc.accent),
        egui::StrokeKind::Inside,
    );

    let text_rect = rect.shrink2(egui::vec2(18.0, 0.0));
    let font = theme::mono(FIELD_FONT_SIZE);
    let before_cursor: String = dialog.value.chars().take(dialog.cursor).collect();
    let cursor_x = painter
        .layout_no_wrap(before_cursor, font.clone(), tc.text)
        .size()
        .x;
    let galley = painter.layout_no_wrap(dialog.value.clone(), font, tc.text);

    // Scroll long names so the cursor always stays inside the field.
    let max_x = text_rect.width() - 4.0;
    let offset = (cursor_x - max_x).max(0.0);
    let origin = egui::pos2(
        text_rect.left() - offset,
        text_rect.center().y - galley.size().y / 2.0,
    );
    let clipped = painter.with_clip_rect(text_rect);
    clipped.galley(origin, galley, tc.text);
    clipped.rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(origin.x + cursor_x, text_rect.center().y - 14.0),
            egui::vec2(2.5, 28.0),
        ),
        1,
        tc.focused_stroke,
    );
}

fn layer_indicator(ui: &mut egui::Ui, layer: KeyboardLayer, tc: &ThemeColors) {
    egui::Frame::new()
        .fill(tc.chip)
        .corner_radius(8)
        .inner_margin(egui::Margin::same(3))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            // Right-to-left parent, so list the layers in reverse.
            for (label, this_layer) in [
                ("#+=", KeyboardLayer::Symbols),
                ("ABC", KeyboardLayer::Uppercase),
                ("abc", KeyboardLayer::Lowercase),
            ] {
                let active = layer == this_layer;
                let (fill, color, font) = if active {
                    (tc.accent, tc.on_accent, theme::bold(13.0))
                } else {
                    (egui::Color32::TRANSPARENT, tc.muted, theme::regular(13.0))
                };
                egui::Frame::new()
                    .fill(fill)
                    .corner_radius(6)
                    .inner_margin(egui::Margin::symmetric(10, 3))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(label).font(font).color(color));
                    });
            }
        });
}

fn on_screen_keyboard(
    ui: &mut egui::Ui,
    dialog: &TextInputDialogState,
    actions: &mut Vec<CommanderAction>,
    tc: &ThemeColors,
) {
    ui.spacing_mut().item_spacing.x = KEY_GAP;
    for (row_index, row) in dialog.keys().iter().enumerate() {
        let padding = ((ui.available_width() - row_width(row)) / 2.0).max(0.0);

        ui.horizontal(|ui| {
            ui.add_space(padding - KEY_GAP);
            for (col_index, key) in row.iter().enumerate() {
                let focused = dialog.focused_row == row_index && dialog.focused_col == col_index;
                if key_button(ui, *key, focused, tc).clicked() {
                    actions.push(CommanderAction::TextInput(action_for_key(*key)));
                }
            }
        });
    }
}

fn key_button(
    ui: &mut egui::Ui,
    key: OnScreenKey,
    focused: bool,
    tc: &ThemeColors,
) -> egui::Response {
    let is_text = matches!(key, OnScreenKey::Text(_));
    let (fill, text_color) = if focused {
        (tc.focused_fill, tc.text)
    } else if key == OnScreenKey::Confirm {
        (tc.accent, tc.on_accent)
    } else if is_text {
        (tc.key, tc.text)
    } else {
        (tc.key_fn, tc.muted)
    };
    let font = match (focused || key == OnScreenKey::Confirm, is_text) {
        (true, true) => theme::bold(20.0),
        (false, true) => theme::regular(20.0),
        (true, false) => theme::bold(15.0),
        (false, false) => theme::regular(15.0),
    };
    let stroke = if focused {
        egui::Stroke::new(2.5, tc.focused_stroke)
    } else {
        egui::Stroke::NONE
    };

    ui.add_sized(
        key_size(key),
        egui::Button::new(
            egui::RichText::new(key_label(key))
                .font(font)
                .color(text_color),
        )
        .fill(fill)
        .stroke(stroke)
        .corner_radius(10),
    )
}

fn action_for_key(key: OnScreenKey) -> TextInputAction {
    match key {
        OnScreenKey::Text(text) => TextInputAction::Insert(text.to_owned()),
        OnScreenKey::Space => TextInputAction::Insert(" ".to_owned()),
        OnScreenKey::Backspace => TextInputAction::Backspace,
        OnScreenKey::Clear => TextInputAction::Clear,
        OnScreenKey::CursorLeft => TextInputAction::CursorLeft,
        OnScreenKey::CursorRight => TextInputAction::CursorRight,
        OnScreenKey::Shift => TextInputAction::ToggleShift,
        OnScreenKey::Symbols => TextInputAction::ToggleSymbols,
        OnScreenKey::Confirm => TextInputAction::Submit,
        OnScreenKey::Cancel => TextInputAction::Cancel,
    }
}

fn key_label(key: OnScreenKey) -> &'static str {
    match key {
        OnScreenKey::Backspace => "Backspace",
        OnScreenKey::CursorLeft => "◀",
        OnScreenKey::CursorRight => "▶",
        OnScreenKey::Symbols => "#+=",
        OnScreenKey::Confirm => "Done",
        _ => key.label(),
    }
}

/// Key sizes keep the same proportions as `OnScreenKey::visual_width`, which drives
/// up/down focus movement between rows.
fn key_size(key: OnScreenKey) -> [f32; 2] {
    let width = match key {
        OnScreenKey::Space => 210.0,
        OnScreenKey::Backspace
        | OnScreenKey::Clear
        | OnScreenKey::Confirm
        | OnScreenKey::Cancel
        | OnScreenKey::Shift
        | OnScreenKey::Symbols => 107.0,
        OnScreenKey::CursorLeft | OnScreenKey::CursorRight => 65.0,
        OnScreenKey::Text(_) => 56.0,
    };
    [width, KEY_HEIGHT]
}

fn row_width(row: &[OnScreenKey]) -> f32 {
    let keys: f32 = row.iter().map(|key| key_size(*key)[0]).sum();
    keys + row.len().saturating_sub(1) as f32 * KEY_GAP
}

fn keyboard_width(dialog: &TextInputDialogState) -> f32 {
    dialog
        .keys()
        .iter()
        .map(|row| row_width(row))
        .fold(0.0, f32::max)
        .max(760.0)
}
