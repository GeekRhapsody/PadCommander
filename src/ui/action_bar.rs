use eframe::egui;

use crate::action::Action;

pub fn show(ui: &mut egui::Ui, actions: &mut Vec<Action>) {
    ui.vertical_centered(|ui| {
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            command_button(ui, actions, "Copy", "Copy");
            command_button(ui, actions, "Move", "Move");
            command_button(ui, actions, "Compress", "Compress");
            command_button(ui, actions, "Decompress", "Decompress");
            command_button(ui, actions, "Settings", "Settings");
            command_button(ui, actions, "Switch Pane", "Switch Pane");

            if ui
                .add_sized([96.0, 36.0], egui::Button::new("Quit"))
                .clicked()
            {
                actions.push(Action::Quit);
            }
        });
    });
}

fn command_button(
    ui: &mut egui::Ui,
    actions: &mut Vec<Action>,
    label: &'static str,
    command: &'static str,
) {
    if ui
        .add_sized([128.0, 36.0], egui::Button::new(label))
        .clicked()
    {
        match command {
            "Switch Pane" => actions.push(Action::SwitchPane),
            command => actions.push(Action::PlaceholderCommand(command)),
        }
    }
}
