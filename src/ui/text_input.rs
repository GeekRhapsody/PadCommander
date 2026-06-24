use eframe::egui;

use crate::{
    action::{CommanderAction, TextInputAction},
    state::{AppState, DialogState, KeyboardLayer, OnScreenKey, TextInputDialogState},
    theme::ThemeColors,
    ui::button_images::{btn, ButtonImages},
};

pub fn show(ctx: &egui::Context, state: &AppState, actions: &mut Vec<CommanderAction>, images: &ButtonImages) {
    let DialogState::TextInput(dialog) = &state.dialog else {
        return;
    };

    let tc = ThemeColors::for_theme(state.settings.theme);
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
        .order(egui::Order::Tooltip)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(tc.text_input_fill)
                .stroke(egui::Stroke::new(1.0, tc.modal_border))
                .inner_margin(egui::Margin::same(18))
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
    ui.set_min_width(720.0);
    ui.heading(&dialog.title);
    ui.add_space(8.0);

    egui::Frame::new()
        .fill(tc.text_input_field_fill)
        .stroke(egui::Stroke::new(1.0, tc.text_input_field_border))
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
    on_screen_keyboard(ui, dialog, actions, tc);
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.set_height(34.0);
        ui.label(layer_label(dialog.layer));
        ui.separator();
        btn(ui, images.dpad_all);
        ui.label("focus");
        ui.separator();
        btn(ui, images.south);
        ui.label("key");
        ui.separator();
        btn(ui, images.east);
        ui.label("cancel");
        ui.separator();
        btn(ui, images.west);
        ui.label("backspace");
        ui.separator();
        btn(ui, images.north);
        ui.label("symbols");
        ui.separator();
        btn(ui, images.lb);
        btn(ui, images.rb);
        ui.label("cursor");
        ui.separator();
        btn(ui, images.start);
        ui.label("confirm");
    });
}

fn on_screen_keyboard(
    ui: &mut egui::Ui,
    dialog: &TextInputDialogState,
    actions: &mut Vec<CommanderAction>,
    tc: &ThemeColors,
) {
    for (row_index, row) in dialog.keys().iter().enumerate() {
        let item_spacing = ui.spacing().item_spacing.x;
        let total_key_width: f32 = row.iter().map(|k| key_size(*k)[0]).sum();
        let gaps = row.len().saturating_sub(1) as f32;
        let row_width = total_key_width + gaps * item_spacing;
        let padding = ((ui.available_width() - row_width) / 2.0).max(0.0);

        ui.horizontal(|ui| {
            ui.add_space(padding);
            for (col_index, key) in row.iter().enumerate() {
                let focused = dialog.focused_row == row_index && dialog.focused_col == col_index;
                let mut button = egui::Button::new(key.label());
                if focused {
                    button = button
                        .fill(tc.focused_fill)
                        .stroke(egui::Stroke::new(2.0, tc.focused_stroke));
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
