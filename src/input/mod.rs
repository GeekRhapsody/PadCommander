use eframe::egui;
use gilrs::{Button, EventType, GamepadId, Gilrs};
use std::time::{Duration, Instant};

use crate::action::CommanderAction;

pub const GAMEPAD_POLL_INTERVAL: Duration = Duration::from_millis(16);

const INITIAL_REPEAT_DELAY: Duration = Duration::from_millis(325);
const REPEAT_INTERVAL: Duration = Duration::from_millis(70);

pub struct InputManager {
    keyboard: KeyboardInputManager,
    gamepad: GamepadInputManager,
}

impl InputManager {
    pub fn new() -> Self {
        Self {
            keyboard: KeyboardInputManager::default(),
            gamepad: GamepadInputManager::new(),
        }
    }

    pub fn collect_actions(&mut self, ctx: &egui::Context) -> Vec<CommanderAction> {
        let now = Instant::now();
        let mut actions = self.keyboard.collect_actions(ctx, now);
        actions.extend(self.gamepad.poll_actions(now));
        actions
    }
}

#[derive(Default)]
struct KeyboardInputManager {
    arrow_up: RepeatState,
    arrow_down: RepeatState,
}

impl KeyboardInputManager {
    fn collect_actions(&mut self, ctx: &egui::Context, now: Instant) -> Vec<CommanderAction> {
        ctx.input(|input| {
            let mut actions = Vec::new();

            actions.extend(self.arrow_up.update(
                input.key_down(egui::Key::ArrowUp),
                now,
                CommanderAction::MoveSelectionUp,
            ));
            actions.extend(self.arrow_down.update(
                input.key_down(egui::Key::ArrowDown),
                now,
                CommanderAction::MoveSelectionDown,
            ));

            if input.key_pressed(egui::Key::Tab) {
                actions.push(CommanderAction::SwitchPane);
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

            if input.key_pressed(egui::Key::Escape) {
                actions.push(CommanderAction::Cancel);
            }

            if input.key_pressed(egui::Key::Backspace) {
                actions.push(CommanderAction::GoParent);
            }

            if input.key_pressed(egui::Key::Space) {
                actions.push(CommanderAction::ToggleMarkSelected);
            }

            if input.key_pressed(egui::Key::F1) {
                actions.push(CommanderAction::ToggleInputHelp);
            }

            if input.key_pressed(egui::Key::F2) {
                actions.push(CommanderAction::Rename);
            }

            if input.key_pressed(egui::Key::F5) {
                actions.push(CommanderAction::CopyToOtherPane);
            }

            if input.key_pressed(egui::Key::F6) {
                actions.push(CommanderAction::MoveToOtherPane);
            }

            if input.key_pressed(egui::Key::F7) {
                actions.push(CommanderAction::CreateFolder);
            }

            if input.key_pressed(egui::Key::F8) || input.key_pressed(egui::Key::Delete) {
                actions.push(CommanderAction::Delete);
            }

            if input.modifiers.ctrl && input.key_pressed(egui::Key::R) {
                actions.push(CommanderAction::Refresh);
            }

            if input.modifiers.ctrl && input.key_pressed(egui::Key::Q) {
                actions.push(CommanderAction::QuitDevelopment);
            }

            actions
        })
    }
}

pub struct GamepadInputManager {
    gilrs: Option<Gilrs>,
    active_gamepad: Option<GamepadId>,
    dpad_up_held: bool,
    dpad_down_held: bool,
    dpad_up: RepeatState,
    dpad_down: RepeatState,
}

impl GamepadInputManager {
    pub fn new() -> Self {
        Self {
            gilrs: Gilrs::new().ok(),
            active_gamepad: None,
            dpad_up_held: false,
            dpad_down_held: false,
            dpad_up: RepeatState::default(),
            dpad_down: RepeatState::default(),
        }
    }

    pub fn poll_actions(&mut self, now: Instant) -> Vec<CommanderAction> {
        let mut actions = Vec::new();

        let events = if let Some(gilrs) = &mut self.gilrs {
            let mut events = Vec::new();
            while let Some(event) = gilrs.next_event() {
                events.push(event);
            }
            gilrs.inc();
            events
        } else {
            Vec::new()
        };

        for event in events {
            self.active_gamepad = Some(event.id);

            match event.event {
                EventType::ButtonPressed(button, _) => {
                    self.set_dpad_hold(button, true);
                    if !matches!(button, Button::DPadUp | Button::DPadDown) {
                        actions.extend(action_for_button(button));
                    }
                }
                EventType::ButtonReleased(button, _) => {
                    self.set_dpad_hold(button, false);
                }
                EventType::Disconnected if self.active_gamepad == Some(event.id) => {
                    self.active_gamepad = None;
                    self.dpad_up_held = false;
                    self.dpad_down_held = false;
                }
                EventType::Disconnected => {}
                EventType::Connected => {
                    self.active_gamepad = Some(event.id);
                }
                _ => {}
            }
        }

        actions.extend(self.dpad_up.update(
            self.dpad_up_held,
            now,
            CommanderAction::MoveSelectionUp,
        ));
        actions.extend(self.dpad_down.update(
            self.dpad_down_held,
            now,
            CommanderAction::MoveSelectionDown,
        ));

        actions
    }

    fn set_dpad_hold(&mut self, button: Button, is_held: bool) {
        match button {
            Button::DPadUp => self.dpad_up_held = is_held,
            Button::DPadDown => self.dpad_down_held = is_held,
            _ => {}
        }
    }
}

fn action_for_button(button: Button) -> Option<CommanderAction> {
    match button {
        Button::DPadLeft | Button::DPadRight | Button::LeftTrigger | Button::RightTrigger => {
            Some(CommanderAction::SwitchPane)
        }
        Button::South => Some(CommanderAction::OpenSelected),
        Button::East => Some(CommanderAction::Cancel),
        Button::West => Some(CommanderAction::ToggleMarkSelected),
        Button::North => Some(CommanderAction::OpenFavourites),
        Button::Start => Some(CommanderAction::OpenSettings),
        Button::Select => Some(CommanderAction::ToggleInputHelp),
        _ => None,
    }
}

#[derive(Default)]
struct RepeatState {
    next_fire_at: Option<Instant>,
}

impl RepeatState {
    fn update(
        &mut self,
        is_held: bool,
        now: Instant,
        action: CommanderAction,
    ) -> Option<CommanderAction> {
        if !is_held {
            self.next_fire_at = None;
            return None;
        }

        match self.next_fire_at {
            None => {
                self.next_fire_at = Some(now + INITIAL_REPEAT_DELAY);
                Some(action)
            }
            Some(next_fire_at) if now >= next_fire_at => {
                self.next_fire_at = Some(now + REPEAT_INTERVAL);
                Some(action)
            }
            Some(_) => None,
        }
    }
}
