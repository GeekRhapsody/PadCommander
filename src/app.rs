use eframe::egui;

use crate::{
    action::CommanderAction,
    input,
    settings::{ControllerOverride, Theme},
    state::AppState,
    theme::ThemeColors,
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
        crate::theme::install_fonts(&cc.egui_ctx);
        configure_visuals(&cc.egui_ctx, state.settings.theme);
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
        configure_visuals(&ctx, self.state.settings.theme);
        self.state.poll_tasks();

        let actions = self.input_manager.collect_actions(
            &ctx,
            input::InputContext {
                text_input: self.state.text_dialog_is_open(),
                settings: self.state.settings_dialog_is_open(),
                favourites: self.state.favourites_dialog_is_open(),
                servers: self.state.ftp_servers_dialog_is_open(),
                modal: self.state.modal_is_active(),
                input_help: self.state.show_input_help,
            },
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

fn configure_visuals(ctx: &egui::Context, theme: Theme) {
    let tc = ThemeColors::for_theme(theme);
    let mut visuals = match theme {
        Theme::Dark => egui::Visuals::dark(),
        Theme::Light => egui::Visuals::light(),
    };
    visuals.panel_fill = tc.bg;
    visuals.window_fill = tc.pane_active;
    visuals.faint_bg_color = tc.chip;
    visuals.extreme_bg_color = tc.field;
    visuals.selection.bg_fill = tc.focused_fill;
    visuals.selection.stroke = egui::Stroke::new(1.5_f32, tc.focused_stroke);
    visuals.hyperlink_color = tc.accent;
    visuals.override_text_color = Some(tc.text);
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, tc.line);
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(8);
    }
    visuals.widgets.inactive.bg_fill = tc.chip;
    visuals.widgets.inactive.weak_bg_fill = tc.chip;
    visuals.widgets.hovered.bg_fill = tc.line;
    visuals.widgets.hovered.weak_bg_fill = tc.line;
    ctx.set_visuals(visuals);
    ctx.all_styles_mut(crate::theme::configure_text_styles);
}
