use eframe::egui;

use crate::{
    action::{CommanderAction, TextInputAction},
    state::{AppState, DialogState, KeyboardLayer, OnScreenKey, TextInputDialogState},
};

pub fn show(ctx: &egui::Context, state: &AppState, actions: &mut Vec<CommanderAction>) {
    let DialogState::TextInput(dialog) = &state.dialog else {
        return;
    };

    let screen_rect = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("text_input_scrim"),
    ));
    painter.rect_filled(
        screen_rect,
        egui::CornerRadius::ZERO,
        egui::Color32::from_black_alpha(190),
    );

    egui::Area::new(egui::Id::new("text_input_dialog"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(35, 39, 40))
                .stroke(egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgb(132, 184, 153),
                ))
                .inner_margin(egui::Margin::same(18))
                .show(ui, |ui| text_input_dialog(ui, dialog, actions));
        });
}

fn text_input_dialog(
    ui: &mut egui::Ui,
    dialog: &TextInputDialogState,
    actions: &mut Vec<CommanderAction>,
) {
    ui.set_min_width(720.0);
    ui.heading(&dialog.title);
    ui.add_space(8.0);

    egui::Frame::new()
        .fill(egui::Color32::from_rgb(24, 27, 28))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(80, 88, 90)))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.add_sized(
                [ui.available_width(), 32.0],
                egui::Label::new(
                    egui::RichText::new(dialog.display_value_with_cursor())
                        .monospace()
                        .size(20.0),
                ),
            );
        });

    if let Some(error) = &dialog.validation_error {
        ui.add_space(6.0);
        ui.label(egui::RichText::new(error).color(egui::Color32::LIGHT_RED));
    }

    ui.add_space(12.0);
    on_screen_keyboard(ui, dialog, actions);
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(layer_label(dialog.layer));
        ui.separator();
        ui.label("D-pad: focus");
        ui.separator();
        ui.label("A: key");
        ui.separator();
        ui.label("B: cancel");
        ui.separator();
        ui.label("X: backspace");
        ui.separator();
        ui.label("Y: layer");
        ui.separator();
        ui.label("Start: confirm");
    });
}

fn on_screen_keyboard(
    ui: &mut egui::Ui,
    dialog: &TextInputDialogState,
    actions: &mut Vec<CommanderAction>,
) {
    for (row_index, row) in dialog.keys().iter().enumerate() {
        ui.horizontal(|ui| {
            for (col_index, key) in row.iter().enumerate() {
                let focused = dialog.focused_row == row_index && dialog.focused_col == col_index;
                let mut button = egui::Button::new(key.label());
                if focused {
                    button = button
                        .fill(egui::Color32::from_rgb(73, 104, 94))
                        .stroke(egui::Stroke::new(2.0, egui::Color32::WHITE));
                }

                if ui.add_sized(key_size(*key), button).clicked() {
                    actions.push(CommanderAction::TextInput(action_for_key(*key)));
                }
            }
        });
        ui.add_space(4.0);
    }
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

fn key_size(key: OnScreenKey) -> [f32; 2] {
    match key {
        OnScreenKey::Space => [180.0, 42.0],
        OnScreenKey::Backspace
        | OnScreenKey::Clear
        | OnScreenKey::Confirm
        | OnScreenKey::Cancel
        | OnScreenKey::Shift
        | OnScreenKey::Symbols => [92.0, 42.0],
        OnScreenKey::CursorLeft | OnScreenKey::CursorRight => [56.0, 42.0],
        OnScreenKey::Text(_) => [48.0, 42.0],
    }
}

fn layer_label(layer: KeyboardLayer) -> &'static str {
    match layer {
        KeyboardLayer::Lowercase => "Layer: lowercase",
        KeyboardLayer::Uppercase => "Layer: uppercase",
        KeyboardLayer::Symbols => "Layer: symbols",
    }
}
