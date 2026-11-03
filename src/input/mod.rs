use eframe::egui;
use gilrs::{Button, EventType, GamepadId, Gilrs};
use std::time::{Duration, Instant};

use crate::{
    action::{CommanderAction, FocusDirection, TextInputAction},
    ui::button_images::ControllerType,
};

pub const GAMEPAD_POLL_INTERVAL: Duration = Duration::from_millis(16);

const INITIAL_REPEAT_DELAY: Duration = Duration::from_millis(325);
const REPEAT_INTERVAL: Duration = Duration::from_millis(70);

/// Which screen or dialog has input focus; decides what each key and button means.
#[derive(Clone, Copy, Debug, Default)]
pub struct InputContext {
    pub text_input: bool,
    pub settings: bool,
    pub favourites: bool,
    pub servers: bool,
    pub modal: bool,
    pub input_help: bool,
}

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

    pub fn controller_type(&self) -> ControllerType {
        self.gamepad.controller_type()
    }

    pub fn collect_actions(
        &mut self,
        ctx: &egui::Context,
        context: InputContext,
    ) -> Vec<CommanderAction> {
        let now = Instant::now();
        let mut actions = self.keyboard.collect_actions(ctx, now, context);
        actions.extend(self.gamepad.poll_actions(now, context));
        actions
    }
}

#[derive(Default)]
struct KeyboardInputManager {
    arrow_up: RepeatState,
    arrow_down: RepeatState,
}

impl KeyboardInputManager {
    fn collect_actions(
        &mut self,
        ctx: &egui::Context,
        now: Instant,
        context: InputContext,
    ) -> Vec<CommanderAction> {
        ctx.input(|input| {
            let mut actions = Vec::new();

            if context.text_input {
                for event in &input.events {
                    if let egui::Event::Text(text) = event
                        && !text.is_empty()
                    {
                        actions.push(CommanderAction::TextInput(TextInputAction::Insert(
                            text.clone(),
                        )));
                    }
                }

                actions.extend(self.arrow_up.update(
                    input.key_down(egui::Key::ArrowUp),
                    now,
                    CommanderAction::TextInput(TextInputAction::MoveKeyFocus(FocusDirection::Up)),
                ));
                actions.extend(self.arrow_down.update(
                    input.key_down(egui::Key::ArrowDown),
                    now,
                    CommanderAction::TextInput(TextInputAction::MoveKeyFocus(FocusDirection::Down)),
                ));

                if input.key_pressed(egui::Key::ArrowLeft) {
                    actions.push(CommanderAction::TextInput(TextInputAction::CursorLeft));
                }

                if input.key_pressed(egui::Key::ArrowRight) {
                    actions.push(CommanderAction::TextInput(TextInputAction::CursorRight));
                }

                if input.key_pressed(egui::Key::Backspace) {
                    actions.push(CommanderAction::TextInput(TextInputAction::Backspace));
                }

                if input.key_pressed(egui::Key::Enter) {
                    actions.push(CommanderAction::TextInput(TextInputAction::Submit));
                }

                if input.key_pressed(egui::Key::Escape) {
                    actions.push(CommanderAction::TextInput(TextInputAction::Cancel));
                }

                if input.modifiers.ctrl && input.key_pressed(egui::Key::Q) {
                    actions.push(CommanderAction::QuitDevelopment);
                }

                return actions;
            }

            if context.input_help {
                if input.key_pressed(egui::Key::Escape) {
                    actions.push(CommanderAction::Cancel);
                }
                if input.key_pressed(egui::Key::F1) {
                    actions.push(CommanderAction::ToggleInputHelp);
                }
                if input.modifiers.ctrl && input.key_pressed(egui::Key::Q) {
                    actions.push(CommanderAction::QuitDevelopment);
                }
                return actions;
            }

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

            if context.settings && input.key_pressed(egui::Key::ArrowLeft) {
                actions.push(CommanderAction::AdjustFocusedSettingLeft);
            }

            if context.settings && input.key_pressed(egui::Key::ArrowRight) {
                actions.push(CommanderAction::AdjustFocusedSettingRight);
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

            if input.key_pressed(egui::Key::F3)
                || (input.modifiers.ctrl && input.key_pressed(egui::Key::D))
            {
                actions.push(CommanderAction::OpenFavourites);
            }

            // Ctrl+F is Total Commander's FTP connect shortcut.
            if input.modifiers.ctrl && input.key_pressed(egui::Key::F) {
                actions.push(CommanderAction::ConnectFtp);
            }

            if input.key_pressed(egui::Key::F9) {
                actions.push(CommanderAction::OpenCommandPalette);
            }

            if input.key_pressed(egui::Key::F10) {
                actions.push(CommanderAction::OpenSettings);
            }

            if input.key_pressed(egui::Key::Insert) {
                if context.favourites {
                    actions.push(CommanderAction::AddFavourite);
                } else if context.servers {
                    actions.push(CommanderAction::AddFtpServer);
                }
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
                actions.push(if context.favourites {
                    CommanderAction::RemoveFavourite
                } else if context.servers {
                    CommanderAction::RemoveFtpServer
                } else {
                    CommanderAction::Delete
                });
            }

            if input.modifiers.ctrl && input.key_pressed(egui::Key::R) {
                actions.push(CommanderAction::Refresh);
            }

            if input.modifiers.ctrl && input.key_pressed(egui::Key::N) {
                actions.push(CommanderAction::CreateFile);
            }

            if input.modifiers.ctrl && input.key_pressed(egui::Key::T) {
                actions.push(CommanderAction::CreateTextFile);
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
    controller_type: ControllerType,
    dpad_up_held: bool,
    dpad_down_held: bool,
    dpad_left_held: bool,
    dpad_right_held: bool,
    dpad_up: RepeatState,
    dpad_down: RepeatState,
    dpad_left: RepeatState,
    dpad_right: RepeatState,
}

impl GamepadInputManager {
    pub fn new() -> Self {
        Self {
            gilrs: Gilrs::new().ok(),
            active_gamepad: None,
            controller_type: ControllerType::default(),
            dpad_up_held: false,
            dpad_down_held: false,
            dpad_left_held: false,
            dpad_right_held: false,
            dpad_up: RepeatState::default(),
            dpad_down: RepeatState::default(),
            dpad_left: RepeatState::default(),
            dpad_right: RepeatState::default(),
        }
    }

    pub fn controller_type(&self) -> ControllerType {
        self.controller_type
    }

    pub fn poll_actions(&mut self, now: Instant, context: InputContext) -> Vec<CommanderAction> {
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
                    let repeated_dpad = if context.text_input || context.settings {
                        matches!(
                            button,
                            Button::DPadUp
                                | Button::DPadDown
                                | Button::DPadLeft
                                | Button::DPadRight
                        )
                    } else {
                        matches!(button, Button::DPadUp | Button::DPadDown)
                    };

                    if !repeated_dpad {
                        actions.extend(action_for_button(button, context));
                    }
                }
                EventType::ButtonReleased(button, _) => {
                    self.set_dpad_hold(button, false);
                }
                EventType::Disconnected if self.active_gamepad == Some(event.id) => {
                    self.active_gamepad = None;
                    self.controller_type = ControllerType::default();
                    self.dpad_up_held = false;
                    self.dpad_down_held = false;
                    self.dpad_left_held = false;
                    self.dpad_right_held = false;
                }
                EventType::Disconnected => {}
                EventType::Connected => {
                    self.active_gamepad = Some(event.id);
                    if let Some(gilrs) = &self.gilrs {
                        let name = gilrs.gamepad(event.id).name().to_owned();
                        self.controller_type = ControllerType::from_gamepad_name(&name);
                    }
                }
                _ => {}
            }
        }

        if context.text_input {
            actions.extend(self.dpad_up.update(
                self.dpad_up_held,
                now,
                CommanderAction::TextInput(TextInputAction::MoveKeyFocus(FocusDirection::Up)),
            ));
            actions.extend(self.dpad_down.update(
                self.dpad_down_held,
                now,
                CommanderAction::TextInput(TextInputAction::MoveKeyFocus(FocusDirection::Down)),
            ));
            actions.extend(self.dpad_left.update(
                self.dpad_left_held,
                now,
                CommanderAction::TextInput(TextInputAction::MoveKeyFocus(FocusDirection::Left)),
            ));
            actions.extend(self.dpad_right.update(
                self.dpad_right_held,
                now,
                CommanderAction::TextInput(TextInputAction::MoveKeyFocus(FocusDirection::Right)),
            ));
        } else if context.settings {
            actions.extend(self.dpad_left.update(
                self.dpad_left_held,
                now,
                CommanderAction::AdjustFocusedSettingLeft,
            ));
            actions.extend(self.dpad_right.update(
                self.dpad_right_held,
                now,
                CommanderAction::AdjustFocusedSettingRight,
            ));
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
        } else if !context.input_help {
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
        }

        actions
    }

    fn set_dpad_hold(&mut self, button: Button, is_held: bool) {
        match button {
            Button::DPadUp => self.dpad_up_held = is_held,
            Button::DPadDown => self.dpad_down_held = is_held,
            Button::DPadLeft => self.dpad_left_held = is_held,
            Button::DPadRight => self.dpad_right_held = is_held,
            _ => {}
        }
    }
}

fn action_for_button(button: Button, context: InputContext) -> Option<CommanderAction> {
    if context.text_input {
        return match button {
            Button::South => Some(CommanderAction::TextInput(TextInputAction::PressFocusedKey)),
            Button::East => Some(CommanderAction::TextInput(TextInputAction::Cancel)),
            Button::West => Some(CommanderAction::TextInput(TextInputAction::Backspace)),
            Button::North => Some(CommanderAction::TextInput(TextInputAction::ToggleSymbols)),
            Button::Start => Some(CommanderAction::TextInput(TextInputAction::Submit)),
            Button::LeftTrigger => Some(CommanderAction::TextInput(TextInputAction::CursorLeft)),
            Button::RightTrigger => Some(CommanderAction::TextInput(TextInputAction::CursorRight)),
            _ => None,
        };
    }

    if context.input_help {
        return match button {
            Button::East => Some(CommanderAction::Cancel),
            Button::Select => Some(CommanderAction::ToggleInputHelp),
            _ => None,
        };
    }

    if context.settings {
        return match button {
            Button::DPadLeft => Some(CommanderAction::AdjustFocusedSettingLeft),
            Button::DPadRight => Some(CommanderAction::AdjustFocusedSettingRight),
            Button::South | Button::Start => Some(CommanderAction::Confirm),
            Button::East => Some(CommanderAction::Cancel),
            _ => None,
        };
    }

    if context.favourites {
        return match button {
            Button::South => Some(CommanderAction::OpenFocusedFavourite),
            Button::East => Some(CommanderAction::Cancel),
            Button::West => Some(CommanderAction::RemoveFavourite),
            Button::North => Some(CommanderAction::AddFavourite),
            _ => None,
        };
    }

    if context.servers {
        return match button {
            Button::South => Some(CommanderAction::Confirm),
            Button::East => Some(CommanderAction::Cancel),
            Button::West => Some(CommanderAction::RemoveFtpServer),
            Button::North => Some(CommanderAction::AddFtpServer),
            _ => None,
        };
    }

    if context.modal {
        return match button {
            Button::South => Some(CommanderAction::OpenSelected),
            Button::East => Some(CommanderAction::Cancel),
            _ => None,
        };
    }

    match button {
        Button::DPadLeft | Button::DPadRight => Some(CommanderAction::SwitchPane),
        Button::LeftTrigger => Some(CommanderAction::MoveSelectionPageUp),
        Button::RightTrigger => Some(CommanderAction::MoveSelectionPageDown),
        Button::South => Some(CommanderAction::OpenSelected),
        Button::East => Some(CommanderAction::GoBack),
        Button::West => Some(CommanderAction::ToggleMarkSelected),
        Button::North => Some(CommanderAction::OpenCommandPalette),
        Button::Start => Some(CommanderAction::OpenSettings),
        Button::Select => Some(CommanderAction::ToggleInputHelp),
        Button::LeftThumb => Some(CommanderAction::OpenFavourites),
        Button::RightThumb => Some(CommanderAction::ConnectFtp),
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
