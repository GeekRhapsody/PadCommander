use eframe::egui;

use crate::{
    action::CommanderAction,
    input,
    settings::ControllerOverride,
    state::AppState,
    ui::button_images::{ButtonImages, ControllerType},
};

pub const APP_TITLE: &str = "PadCommander";

pub struct PadCommanderApp {
    state: AppState,
    input_manager: input::InputManager,
}

impl PadCommanderApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let state = AppState::new();
        apply_ui_scale(&cc.egui_ctx, state.settings.ui_scale);
        configure_visuals(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);

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

impl eframe::App for PadCommanderApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        apply_ui_scale(&ctx, self.state.settings.ui_scale);
        self.state.poll_tasks();

        let actions = self.input_manager.collect_actions(
            &ctx,
            self.state.text_dialog_is_open(),
            self.state.settings_dialog_is_open(),
            self.state.modal_is_active(),
            self.state.show_input_help,
        );
        for action in actions {
            self.handle_action(&ctx, action);
        }

        let effective_ct = match self.state.settings.controller_override {
            ControllerOverride::Auto => self.input_manager.controller_type(),
            ControllerOverride::Xbox => ControllerType::Xbox,
            ControllerOverride::PlayStation4 => ControllerType::PlayStation4,
            ControllerOverride::PlayStation5 => ControllerType::PlayStation5,
            ControllerOverride::NintendoSwitch => ControllerType::NintendoSwitch,
        };
        let button_images = ButtonImages::for_controller(effective_ct);
        for action in crate::ui::show(ui, &self.state, &button_images) {
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
    visuals.override_text_color = Some(egui::Color32::WHITE);
    ctx.set_visuals(visuals);
}
