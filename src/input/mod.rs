use eframe::egui;

use crate::action::Action;

pub fn collect_actions(ctx: &egui::Context) -> Vec<Action> {
    ctx.input(|input| {
        let mut actions = Vec::new();

        if input.key_pressed(egui::Key::Tab) {
            actions.push(Action::SwitchPane);
        }

        if input.key_pressed(egui::Key::Escape)
            || (input.modifiers.ctrl && input.key_pressed(egui::Key::Q))
        {
            actions.push(Action::Quit);
        }

        actions
    })
}
