use eframe::egui;

use crate::action::CommanderAction;

pub fn collect_actions(ctx: &egui::Context) -> Vec<CommanderAction> {
    ctx.input(|input| {
        let mut actions = Vec::new();

        if input.key_pressed(egui::Key::Tab) {
            actions.push(CommanderAction::SwitchPane);
        }

        if input.key_pressed(egui::Key::ArrowUp) {
            actions.push(CommanderAction::MoveSelectionUp);
        }

        if input.key_pressed(egui::Key::ArrowDown) {
            actions.push(CommanderAction::MoveSelectionDown);
        }

        if input.key_pressed(egui::Key::PageUp) {
            actions.push(CommanderAction::MoveSelectionPageUp);
        }

        if input.key_pressed(egui::Key::PageDown) {
            actions.push(CommanderAction::MoveSelectionPageDown);
        }

        if input.key_pressed(egui::Key::Home) {
            actions.push(CommanderAction::MoveSelectionHome);
        }

        if input.key_pressed(egui::Key::End) {
            actions.push(CommanderAction::MoveSelectionEnd);
        }

        if input.key_pressed(egui::Key::Enter) {
            actions.push(CommanderAction::OpenSelected);
        }

        if input.key_pressed(egui::Key::Backspace) {
            actions.push(CommanderAction::GoParent);
        }

        if input.key_pressed(egui::Key::Escape)
            || (input.modifiers.ctrl && input.key_pressed(egui::Key::Q))
        {
            actions.push(CommanderAction::QuitDevelopment);
        }

        actions
    })
}
