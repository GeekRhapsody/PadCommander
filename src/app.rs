use eframe::egui;

use crate::{action::CommanderAction, input, state::AppState};

pub const APP_TITLE: &str = "Handheld Commander";

pub struct HandheldCommanderApp {
    state: AppState,
    input_manager: input::InputManager,
}

impl HandheldCommanderApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let state = AppState::new();
        apply_ui_scale(&cc.egui_ctx, state.settings.ui_scale);
        configure_visuals(&cc.egui_ctx);

        Self {
            state,
            input_manager: input::InputManager::new(),
        }
    }

    fn handle_action(&mut self, ctx: &egui::Context, action: CommanderAction) {
        let result = self.state.dispatch_action(action);

        if result.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for HandheldCommanderApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        apply_ui_scale(&ctx, self.state.settings.ui_scale);
        self.state.poll_tasks();

        let mut actions = self.input_manager.collect_actions(
            &ctx,
            self.state.text_dialog_is_open(),
            self.state.settings_dialog_is_open(),
        );
        actions.extend(crate::ui::show(ui, &self.state));

        for action in actions {
            self.handle_action(&ctx, action);
        }

        ctx.request_repaint_after(input::GAMEPAD_POLL_INTERVAL);
    }
}

fn apply_ui_scale(ctx: &egui::Context, ui_scale: f32) {
    ctx.set_pixels_per_point(ui_scale.max(0.5));
}

fn configure_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = egui::Color32::from_rgb(24, 26, 28);
    visuals.window_fill = egui::Color32::from_rgb(31, 33, 36);
    visuals.faint_bg_color = egui::Color32::from_rgb(37, 41, 42);
    visuals.selection.bg_fill = egui::Color32::from_rgb(66, 96, 89);
    visuals.hyperlink_color = egui::Color32::from_rgb(132, 184, 153);
    ctx.set_visuals(visuals);
}
