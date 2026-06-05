use eframe::egui;

use crate::action::CommanderAction;

pub fn show(ui: &mut egui::Ui, actions: &mut Vec<CommanderAction>) {
    ui.vertical_centered(|ui| {
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            command_button(ui, actions, "New Folder", CommanderAction::CreateFolder);
            command_button(ui, actions, "New File", CommanderAction::CreateFile);
            command_button(ui, actions, "Text File", CommanderAction::CreateTextFile);
            command_button(ui, actions, "Rename", CommanderAction::Rename);
            command_button(ui, actions, "Delete", CommanderAction::Delete);
            command_button(ui, actions, "Copy", CommanderAction::CopyToOtherPane);
            command_button(ui, actions, "Move", CommanderAction::MoveToOtherPane);
            command_button(
                ui,
                actions,
                "Compress",
                CommanderAction::CompressToOtherPane,
            );
            command_button(
                ui,
                actions,
                "Decompress",
                CommanderAction::DecompressToOtherPane,
            );
            command_button(ui, actions, "Settings", CommanderAction::OpenSettings);
            command_button(ui, actions, "Switch Pane", CommanderAction::SwitchPane);

            if ui
                .add_sized([96.0, 36.0], egui::Button::new("Quit"))
                .clicked()
            {
                actions.push(CommanderAction::QuitDevelopment);
            }
        });
    });
}

fn command_button(
    ui: &mut egui::Ui,
    actions: &mut Vec<CommanderAction>,
    label: &'static str,
    action: CommanderAction,
) {
    if ui
        .add_sized([128.0, 36.0], egui::Button::new(label))
        .clicked()
    {
        actions.push(action);
    }
}
