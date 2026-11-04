use crate::{
    action::{CommanderAction, TextInputAction, TextInputPurpose},
    fs::{self, LocalFileSystem, VirtualFileSystem},
    ftp::{self, FtpConnection, FtpServer},
    platform, secrets,
    settings::{self, Settings, UI_SCALE_STEP},
    tasks::{DeleteMode, TaskId, TaskKind, TaskManager, TaskOutcome, TaskStatus},
    update::{self, Update},
};

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    thread,
};

mod dialogs;
mod pane;
mod text_input;

pub use dialogs::*;
use dialogs::{DecompressRequest, PendingShortcutRequest, ShortcutRequest};
pub use pane::*;
pub use text_input::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum PaneSide {
    Left,
    Right,
}

impl PaneSide {
    fn label(self) -> &'static str {
        match self {
            PaneSide::Left => "Left",
            PaneSide::Right => "Right",
        }
    }

    fn other(self) -> Self {
        match self {
            PaneSide::Left => PaneSide::Right,
            PaneSide::Right => PaneSide::Left,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PaneSortOrder {
    #[default]
    FilenameAsc,
    FilenameDesc,
    DateAsc,
    DateDesc,
    SizeAsc,
    SizeDesc,
}

impl PaneSortOrder {
    pub const COUNT: usize = 6;

    pub fn label(self) -> &'static str {
        match self {
            Self::FilenameAsc => "Filename (A to Z)",
            Self::FilenameDesc => "Filename (Z to A)",
            Self::DateAsc => "Date (Oldest to Newest)",
            Self::DateDesc => "Date (Newest to Oldest)",
            Self::SizeAsc => "Size (Smallest to Largest)",
            Self::SizeDesc => "Size (Largest to Smallest)",
        }
    }
}

pub struct AppState {
    pub settings: Settings,
    pub settings_path: PathBuf,
    pub left_pane: PaneState,
    pub right_pane: PaneState,
    pub active_pane: PaneSide,
    pub status: String,
    pub dialog: DialogState,
    pub show_input_help: bool,
    pub local_file_system: LocalFileSystem,
    pub task_manager: TaskManager,
    pub delete_tasks: BTreeMap<u64, DeleteTaskContext>,
    pub transfer_task_panes: BTreeMap<TaskId, Vec<PaneSide>>,
    pending_shortcut_request: Option<PendingShortcutRequest>,
    pending_decompress_request: Option<DecompressRequest>,
    pending_ftp_server: Option<FtpServer>,
    ftp_sessions: BTreeMap<PaneSide, FtpSession>,
    ftp_events_tx: mpsc::Sender<FtpEvent>,
    ftp_events_rx: mpsc::Receiver<FtpEvent>,
    update_check: Option<mpsc::Receiver<Result<Option<Update>, String>>>,
    update_task: Option<(TaskId, Update)>,
}

/// A pane connected to an FTP server. Dropped as soon as the pane shows a local location.
struct FtpSession {
    connection: Arc<FtpConnection>,
    return_location: String,
    offer_save_password: bool,
}

/// Results of FTP work run on background threads, drained in `poll_tasks`.
enum FtpEvent {
    Listed {
        side: PaneSide,
        location: String,
        result: Result<Vec<FileEntry>, String>,
    },
    OperationDone {
        side: PaneSide,
        result: Result<String, String>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeleteTaskContext {
    pub pane: PaneSide,
    pub mode: DeleteMode,
}

impl AppState {
    pub fn new() -> Self {
        let local_file_system = LocalFileSystem;
        let initial_location = local_file_system.initial_location();
        let settings_path = settings::config_file_path();
        let mut settings = settings::load().unwrap_or_default().sanitize();
        let left_location = settings
            .last_left_pane_location
            .clone()
            .filter(|location| fs::location_exists(location))
            .unwrap_or_else(|| fs::default_open_location(&initial_location));
        let right_location = settings
            .last_right_pane_location
            .clone()
            .filter(|location| fs::location_exists(location))
            .unwrap_or_else(|| fs::default_open_location(&initial_location));
        settings.last_left_pane_location = Some(left_location.clone());
        settings.last_right_pane_location = Some(right_location.clone());
        let (ftp_events_tx, ftp_events_rx) = mpsc::channel();
        let mut state = Self {
            settings,
            settings_path,
            left_pane: PaneState::new(left_location),
            right_pane: PaneState::new(right_location),
            active_pane: PaneSide::Left,
            status: "Ready. F1 or View opens input help. Ctrl+Q quits during development."
                .to_owned(),
            dialog: DialogState::None,
            show_input_help: false,
            local_file_system,
            task_manager: TaskManager::default(),
            delete_tasks: BTreeMap::new(),
            transfer_task_panes: BTreeMap::new(),
            pending_shortcut_request: None,
            pending_decompress_request: None,
            pending_ftp_server: None,
            ftp_sessions: BTreeMap::new(),
            ftp_events_tx,
            ftp_events_rx,
            update_check: None,
            update_task: None,
        };

        state.refresh_pane(PaneSide::Left);
        state.refresh_pane(PaneSide::Right);
        state
    }

    pub fn active_pane_label(&self) -> &'static str {
        self.active_pane.label()
    }

    pub fn dispatch_action(&mut self, action: CommanderAction) -> DispatchResult {
        match action {
            CommanderAction::ActivatePane(side) => {
                self.active_pane = side;
                self.status = format!("{} pane active.", self.active_pane_label());
            }
            CommanderAction::SelectEntry { pane, index } => {
                self.active_pane = pane;
                self.pane_mut(pane).select_index(index);
                self.status = format!("{} pane active.", self.active_pane_label());
            }
            CommanderAction::SwitchPane => {
                if matches!(self.dialog, DialogState::None) {
                    self.switch_active_pane();
                    self.status = format!("{} pane active.", self.active_pane_label());
                }
            }
            CommanderAction::MoveSelectionUp => {
                if self.move_command_menu_focus(SelectionMovement::Up) {
                    self.status = "Command menu focus moved.".to_owned();
                } else if matches!(self.dialog, DialogState::None) {
                    self.move_active_selection(SelectionMovement::Up);
                }
            }
            CommanderAction::MoveSelectionDown => {
                if self.move_command_menu_focus(SelectionMovement::Down) {
                    self.status = "Command menu focus moved.".to_owned();
                } else if matches!(self.dialog, DialogState::None) {
                    self.move_active_selection(SelectionMovement::Down)
                }
            }
            CommanderAction::MoveSelectionPageUp => {
                if matches!(self.dialog, DialogState::None) {
                    self.move_active_selection(SelectionMovement::PageUp)
                }
            }
            CommanderAction::MoveSelectionPageDown => {
                if matches!(self.dialog, DialogState::None) {
                    self.move_active_selection(SelectionMovement::PageDown);
                }
            }
            CommanderAction::MoveSelectionHome => {
                self.move_active_selection(SelectionMovement::Home)
            }
            CommanderAction::MoveSelectionEnd => self.move_active_selection(SelectionMovement::End),
            CommanderAction::OpenSelected => {
                if self.dialog != DialogState::None {
                    return self.confirm_dialog();
                } else {
                    self.open_or_confirm_selected();
                }
            }
            CommanderAction::GoBack => {
                self.go_back();
            }
            CommanderAction::GoParent => {
                self.go_parent(self.active_pane);
            }
            CommanderAction::SetPaneScrollOffset { pane, offset } => {
                let pane = self.pane_mut(pane);
                pane.scroll_offset = offset.max(0) as f32;
                pane.needs_scroll_to_selected = false;
            }
            CommanderAction::Refresh => {
                self.refresh_pane(self.active_pane);
            }
            CommanderAction::ToggleMarkSelected => {
                if matches!(self.dialog, DialogState::Favourites(_)) {
                    self.remove_focused_favourite();
                } else if matches!(self.dialog, DialogState::FtpServers(_)) {
                    self.remove_focused_ftp_server();
                } else {
                    self.toggle_mark_selected();
                }
            }
            CommanderAction::Cancel => {
                if self.show_input_help {
                    self.show_input_help = false;
                    self.status = "Input help closed.".to_owned();
                } else if self.text_dialog_is_open() {
                    self.handle_text_input(TextInputAction::Cancel);
                } else if let DialogState::OperationProgress(task_id) = self.dialog {
                    let done = self
                        .task_manager
                        .task_by_id(task_id)
                        .is_none_or(|t| t.status.is_finished());
                    if done {
                        self.dialog = DialogState::None;
                    } else {
                        self.cancel_active_task();
                    }
                } else if self.dialog != DialogState::None {
                    self.dialog = DialogState::None;
                    self.status = "Cancelled.".to_owned();
                } else if self.clear_active_error() {
                    self.status = "Error dismissed.".to_owned();
                } else if !self.cancel_active_task() {
                    self.status = "Cancel/back is reserved for navigation history.".to_owned();
                }
            }
            CommanderAction::Confirm => return self.confirm_dialog(),
            CommanderAction::OpenSettings => {
                self.dialog = DialogState::Settings(SettingsModalState::default());
                self.status = "Settings opened.".to_owned();
            }
            CommanderAction::CheckForUpdates => {
                let (sender, receiver) = mpsc::channel();
                thread::spawn(move || {
                    let _ = sender.send(update::check_latest());
                });
                self.update_check = Some(receiver);
                self.dialog = DialogState::Message {
                    title: "Check for Updates".to_owned(),
                    message: "Looking for a newer version...".to_owned(),
                };
                self.status = "Checking for updates...".to_owned();
            }
            CommanderAction::CreateFolder => {
                self.begin_text_input(TextInputPurpose::CreateFolder, String::new())
            }
            CommanderAction::CreateFile => {
                self.begin_text_input(TextInputPurpose::CreateFile, String::new())
            }
            CommanderAction::CreateTextFile => {
                self.begin_text_input(TextInputPurpose::CreateTextFile, String::new())
            }
            CommanderAction::Rename => {
                let initial_value = self
                    .active_pane()
                    .selected_markable_entry()
                    .map(|entry| entry.display_name.clone())
                    .unwrap_or_default();
                self.begin_text_input(TextInputPurpose::Rename, initial_value);
            }
            CommanderAction::ConnectFtp => {
                self.dialog = DialogState::FtpServers(ListModalState::default());
                self.status = "Servers opened.".to_owned();
            }
            CommanderAction::AddFtpServer => {
                self.begin_text_input(TextInputPurpose::ConnectFtp, "ftp://".to_owned())
            }
            CommanderAction::RemoveFtpServer => self.remove_focused_ftp_server(),
            CommanderAction::FocusFtpServer(index) => {
                if let DialogState::FtpServers(list) = &mut self.dialog {
                    list.focused_index = index.min(self.settings.ftp_servers.len());
                }
            }
            CommanderAction::DisconnectFtp => {
                let Some(session) = self.ftp_sessions.get(&self.active_pane) else {
                    self.status = "This pane is not connected to a server.".to_owned();
                    return DispatchResult::default();
                };
                let location = session.return_location.clone();
                self.navigate_active_pane(location);
                self.status = "Disconnected.".to_owned();
            }
            CommanderAction::CompressToOtherPane | CommanderAction::DecompressToOtherPane
                if !self.ftp_sessions.is_empty() =>
            {
                self.status = "Archives are not available while a pane is on a server.".to_owned();
            }
            CommanderAction::CompressToOtherPane => self.begin_archive_name_input(),
            CommanderAction::DecompressToOtherPane => self.begin_archive_decompression_choices(),
            CommanderAction::OpenFavourites => {
                self.dialog = DialogState::Favourites(ListModalState::default());
                self.status = "Favourites opened.".to_owned();
            }
            CommanderAction::AddFavourite => self.add_active_location_to_favourites(),
            CommanderAction::RemoveFavourite => self.remove_focused_favourite(),
            CommanderAction::OpenFocusedFavourite => self.open_focused_favourite(),
            CommanderAction::FocusFavourite(index) => self.focus_favourite(index),
            CommanderAction::FocusDecompressOption(index) => self.focus_decompress_option(index),
            CommanderAction::IncreaseUiScale => {
                if matches!(self.dialog, DialogState::Settings(_)) {
                    self.adjust_ui_scale(UI_SCALE_STEP);
                }
            }
            CommanderAction::DecreaseUiScale => {
                if matches!(self.dialog, DialogState::Settings(_)) {
                    self.adjust_ui_scale(-UI_SCALE_STEP);
                }
            }
            CommanderAction::AdjustFocusedSettingLeft => {
                if matches!(self.dialog, DialogState::Settings(_)) {
                    self.adjust_focused_setting(-UI_SCALE_STEP);
                }
            }
            CommanderAction::AdjustFocusedSettingRight => {
                if matches!(self.dialog, DialogState::Settings(_)) {
                    self.adjust_focused_setting(UI_SCALE_STEP);
                }
            }
            CommanderAction::ToggleUseRecycleBin => {
                if matches!(self.dialog, DialogState::Settings(_)) {
                    self.toggle_use_recycle_bin();
                }
            }
            CommanderAction::ToggleShowHiddenFiles => {
                if matches!(self.dialog, DialogState::Settings(_)) {
                    self.toggle_show_hidden_files();
                }
            }
            CommanderAction::OpenCommandPalette => {
                if matches!(self.dialog, DialogState::Favourites(_)) {
                    self.add_active_location_to_favourites();
                } else {
                    self.dialog = DialogState::CommandMenu(CommandMenuState::new(
                        self.can_create_shortcut_in_other_pane(),
                        self.ftp_sessions.is_empty(),
                        self.ftp_sessions.contains_key(&self.active_pane),
                    ));
                    self.status = "Command menu opened.".to_owned();
                }
            }
            CommanderAction::ToggleInputHelp => {
                self.show_input_help = !self.show_input_help;
                self.status = if self.show_input_help {
                    "Input help opened.".to_owned()
                } else {
                    "Input help closed.".to_owned()
                };
            }
            CommanderAction::TextInput(text_action) => self.handle_text_input(text_action),
            CommanderAction::QuitDevelopment => {
                self.status = "Quit requested.".to_owned();
                return DispatchResult {
                    quit_requested: true,
                };
            }
            CommanderAction::Delete => self.begin_delete_confirmation(),
            CommanderAction::CopyToOtherPane => self.start_transfer_task(TaskKind::Copy),
            CommanderAction::MoveToOtherPane => self.start_transfer_task(TaskKind::Move),
            CommanderAction::CreateShortcutInOtherPane => {
                self.begin_create_shortcut_in_other_pane()
            }
            CommanderAction::OpenSortBy => {
                self.dialog =
                    DialogState::SortBy(SortByModalState::new(self.active_pane().sort_order));
                self.status = "Sort options opened.".to_owned();
            }
            CommanderAction::SelectSortOrder(sort_order) => self.apply_active_pane_sort(sort_order),
            CommanderAction::StartDemoTask => self.start_demo_task(false),
            CommanderAction::StartFailingDemoTask => self.start_demo_task(true),
            CommanderAction::CancelActiveTask => {
                if !self.cancel_active_task() {
                    self.status = "No cancellable task is active.".to_owned();
                }
            }
        }

        DispatchResult::default()
    }

    pub fn poll_tasks(&mut self) {
        while let Ok(event) = self.ftp_events_rx.try_recv() {
            match event {
                FtpEvent::Listed {
                    side,
                    location,
                    result,
                } => {
                    if self.pane(side).current_location != location {
                        continue;
                    }
                    let listed = result.is_ok();
                    self.apply_listing(side, &location, result);

                    let Some(session) = self.ftp_sessions.get_mut(&side) else {
                        continue;
                    };
                    if !listed || !session.offer_save_password || self.dialog != DialogState::None {
                        continue;
                    }
                    session.offer_save_password = false;
                    self.dialog = DialogState::SavePassword(SavePasswordState {
                        side,
                        server_label: session.connection.server.label(),
                        focused_index: 0,
                    });
                }
                FtpEvent::OperationDone { side, result } => match result {
                    Ok(message) => {
                        self.refresh_pane(side);
                        self.status = message;
                    }
                    Err(error) => self.status = error,
                },
            }
        }

        if let Some(receiver) = &self.update_check
            && let Ok(result) = receiver.try_recv()
        {
            self.update_check = None;
            self.dialog = match result {
                Ok(Some(update)) => DialogState::UpdateAvailable(UpdatePromptState {
                    update,
                    focused_index: 0,
                }),
                Ok(None) => DialogState::Message {
                    title: "Up to Date".to_owned(),
                    message: format!(
                        "PadCommander {} is the latest version.",
                        update::CURRENT_VERSION
                    ),
                },
                Err(error) => DialogState::Message {
                    title: "Update Check Failed".to_owned(),
                    message: error,
                },
            };
            self.status = "Update check finished.".to_owned();
        }

        for completion in self.task_manager.poll_completed() {
            let modal_is_showing =
                matches!(self.dialog, DialogState::OperationProgress(id) if id == completion.id);

            if let Some((_, update)) = self
                .update_task
                .take_if(|(id, _)| *id == completion.id)
                .filter(|_| completion.status == TaskStatus::Succeeded)
            {
                // Leave any other dialog the user opened during the download alone.
                if modal_is_showing || self.dialog == DialogState::None {
                    self.dialog = DialogState::UpdateInstalled(UpdatePromptState {
                        update,
                        focused_index: 0,
                    });
                } else {
                    self.status = format!(
                        "Version {} installed. Restart PadCommander to use it.",
                        update.version
                    );
                }
                continue;
            }

            if let Some(context) = self.delete_tasks.remove(&completion.id) {
                self.refresh_pane(context.pane);
                if !modal_is_showing {
                    match completion.result {
                        Ok(data) if completion.status == TaskStatus::Succeeded => {
                            self.status = context.mode.success_message(data.item_count);
                        }
                        Err(error) => {
                            self.status = if completion.status == TaskStatus::Cancelled {
                                format!("{} cancelled.", completion.label)
                            } else {
                                format!("{} failed: {error}", completion.label)
                            };
                        }
                        Ok(_) => {}
                    }
                }
            } else if let Some(sides) = self.transfer_task_panes.remove(&completion.id) {
                for side in sides {
                    self.refresh_pane(side);
                }

                if !modal_is_showing {
                    match completion.status {
                        TaskStatus::Succeeded => {
                            self.status = format!("{} completed.", completion.label);
                        }
                        TaskStatus::Failed => {
                            let error = completion.error_message().unwrap_or("unknown error");
                            self.status = format!("{} failed: {error}", completion.label);
                        }
                        TaskStatus::Cancelled => {
                            self.status = format!("{} cancelled.", completion.label);
                        }
                        TaskStatus::Queued | TaskStatus::Running => {}
                    }
                }
            } else if !modal_is_showing {
                match completion.status {
                    TaskStatus::Succeeded => {
                        self.status = format!("{} completed.", completion.label);
                    }
                    TaskStatus::Failed => {
                        let error = completion.error_message().unwrap_or("unknown error");
                        self.status = format!("{} failed: {error}", completion.label);
                    }
                    TaskStatus::Cancelled => {
                        self.status = format!("{} cancelled.", completion.label);
                    }
                    TaskStatus::Queued | TaskStatus::Running => {}
                }
            }
        }
    }

    pub fn active_pane(&self) -> &PaneState {
        self.pane(self.active_pane)
    }

    fn pane(&self, side: PaneSide) -> &PaneState {
        match side {
            PaneSide::Left => &self.left_pane,
            PaneSide::Right => &self.right_pane,
        }
    }

    fn pane_mut(&mut self, side: PaneSide) -> &mut PaneState {
        match side {
            PaneSide::Left => &mut self.left_pane,
            PaneSide::Right => &mut self.right_pane,
        }
    }

    fn inactive_pane_side(&self) -> PaneSide {
        match self.active_pane {
            PaneSide::Left => PaneSide::Right,
            PaneSide::Right => PaneSide::Left,
        }
    }

    fn switch_active_pane(&mut self) {
        self.active_pane = match self.active_pane {
            PaneSide::Left => PaneSide::Right,
            PaneSide::Right => PaneSide::Left,
        };
    }

    fn move_active_selection(&mut self, movement: SelectionMovement) {
        let active_side = self.active_pane;
        let pane = self.pane_mut(active_side);
        pane.move_selection(movement);
        let selected_name = pane
            .selected_entry()
            .map(|entry| entry.display_name.as_str())
            .unwrap_or("nothing");
        self.status = format!(
            "{} pane selected {selected_name}.",
            match active_side {
                PaneSide::Left => "Left",
                PaneSide::Right => "Right",
            }
        );
    }

    fn open_or_confirm_selected(&mut self) {
        if self.dialog != DialogState::None {
            self.dialog = DialogState::None;
            self.status = "Confirmed placeholder dialog.".to_owned();
            return;
        }

        let Some(selected_entry) = self.active_pane().selected_entry().cloned() else {
            self.status = "No entry selected.".to_owned();
            return;
        };

        match selected_entry.kind {
            FileEntryKind::Drive | FileEntryKind::Directory | FileEntryKind::Parent => {
                self.navigate_active_pane(selected_entry.virtual_path);
            }
            FileEntryKind::File | FileEntryKind::Archive => {
                let path = std::path::Path::new(&selected_entry.virtual_path);
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_ascii_lowercase());
                let is_executable = matches!(ext.as_deref(), Some("exe" | "bat" | "cmd" | "com"));

                if is_executable {
                    let parent = path.parent().unwrap_or(path);
                    match std::process::Command::new(path).current_dir(parent).spawn() {
                        Ok(_) => {
                            self.status = format!("Launched {}.", selected_entry.display_name);
                        }
                        Err(e) => {
                            self.status =
                                format!("Failed to launch {}: {}", selected_entry.display_name, e);
                        }
                    }
                } else {
                    match platform::open_with_default_app(path) {
                        Ok(()) => {
                            self.status = format!("Opened {}.", selected_entry.display_name);
                        }
                        Err(e) => {
                            self.status =
                                format!("Failed to open {}: {}", selected_entry.display_name, e);
                        }
                    }
                }
            }
            FileEntryKind::FtpItem => {
                self.status = format!(
                    "Copy {} to a local pane to open it.",
                    selected_entry.display_name
                );
            }
            FileEntryKind::Unknown => {
                self.status = format!("{} cannot be opened.", selected_entry.display_name);
            }
        }
    }

    fn toggle_mark_selected(&mut self) {
        let active_pane = self.active_pane;
        let Some((path, display_name)) = self
            .pane(active_pane)
            .selected_markable_entry()
            .map(|entry| (entry.virtual_path.clone(), entry.display_name.clone()))
        else {
            self.status = "The focused row cannot be marked.".to_owned();
            return;
        };

        let pane = self.pane_mut(active_pane);
        let marked = if pane.marked_items.contains(&path) {
            pane.marked_items.remove(&path);
            false
        } else {
            pane.marked_items.insert(path.clone());
            true
        };

        self.status = if marked {
            format!("Marked {display_name}.")
        } else {
            format!("Unmarked {display_name}.")
        };
    }

    fn handle_text_input(&mut self, text_action: TextInputAction) {
        match text_action {
            TextInputAction::Begin {
                purpose,
                initial_value,
            } => self.begin_text_input(purpose, initial_value),
            TextInputAction::Cancel => {
                self.clear_pending_text_input_state();
                self.dialog = DialogState::None;
                self.status = "Text input cancelled.".to_owned();
            }
            action => {
                let Some(dialog) = self.text_dialog_mut() else {
                    self.status = "Text entry is not active.".to_owned();
                    return;
                };

                match dialog.apply_action(action) {
                    TextInputOutcome::Editing => {
                        self.status = "Editing text.".to_owned();
                    }
                    TextInputOutcome::Cancelled => {
                        self.clear_pending_text_input_state();
                        self.dialog = DialogState::None;
                        self.status = "Text input cancelled.".to_owned();
                    }
                    TextInputOutcome::Blocked(error) => {
                        self.status = error;
                    }
                    TextInputOutcome::Submitted { purpose, value } => {
                        self.dialog = DialogState::None;
                        self.apply_text_submission(purpose, value);
                    }
                }
            }
        }
    }

    fn begin_text_input(&mut self, purpose: TextInputPurpose, initial_value: String) {
        let title = purpose.title();
        self.dialog = DialogState::TextInput(TextInputDialogState::new(purpose, initial_value));
        self.status = format!("{title} text input opened.");
    }

    fn confirm_dialog(&mut self) -> DispatchResult {
        match &self.dialog {
            DialogState::TextInput(_) => self.handle_text_input(TextInputAction::Submit),
            DialogState::DeleteConfirm(confirm) => {
                if confirm.focused_index == 1 {
                    self.dialog = DialogState::None;
                    return DispatchResult::default();
                }
                let confirm = confirm.clone();
                self.start_delete_task(confirm);
            }
            DialogState::ShortcutConflict(confirm) => {
                if confirm.focused_index == 1 {
                    self.dialog = DialogState::None;
                    return DispatchResult::default();
                }
                let confirm = confirm.clone();
                self.start_create_shortcut(confirm.request, true);
            }
            DialogState::TransferConflict(conflict) => {
                if conflict.focused_index == 1 {
                    self.dialog = DialogState::None;
                    return DispatchResult::default();
                }
                let conflict = conflict.clone();
                self.dialog = DialogState::None;
                self.execute_transfer_task(
                    conflict.kind,
                    conflict.source_side,
                    conflict.target_side,
                    conflict.sources,
                    conflict.target_location,
                    conflict.label,
                    true,
                );
            }
            DialogState::CommandMenu(menu) => {
                let action = menu.selected_action();
                self.dialog = DialogState::None;
                return self.dispatch_action(action);
            }
            DialogState::Settings(_) => return self.activate_focused_setting(),
            DialogState::Favourites(_) => self.open_focused_favourite(),
            DialogState::FtpServers(list) => {
                let Some(server) = self.settings.ftp_servers.get(list.focused_index).cloned()
                else {
                    return self.dispatch_action(CommanderAction::AddFtpServer);
                };
                self.dialog = DialogState::None;
                let saved_password = server
                    .saved_password
                    .as_deref()
                    .map(|sealed| secrets::open(&self.config_dir(), sealed));
                match saved_password {
                    _ if server.is_anonymous() => self.connect_ftp(server, String::new(), false),
                    Some(Ok(password)) => self.connect_ftp(server, password, false),
                    Some(Err(error)) => {
                        self.pending_ftp_server = Some(server);
                        self.begin_text_input(TextInputPurpose::FtpPassword, String::new());
                        self.status = format!("{error} Enter the password.");
                    }
                    None => {
                        self.pending_ftp_server = Some(server);
                        self.begin_text_input(TextInputPurpose::FtpPassword, String::new());
                    }
                }
            }
            DialogState::SavePassword(prompt) => {
                let prompt = prompt.clone();
                self.dialog = DialogState::None;
                if prompt.focused_index == 1 {
                    self.status = "Password not saved.".to_owned();
                    return DispatchResult::default();
                }
                let Some(password) = self
                    .ftp_sessions
                    .get(&prompt.side)
                    .map(|session| session.connection.password.clone())
                else {
                    return DispatchResult::default();
                };
                let sealed = match secrets::seal(&self.config_dir(), &password) {
                    Ok(sealed) => sealed,
                    Err(error) => {
                        self.status = error;
                        return DispatchResult::default();
                    }
                };
                if let Some(server) = self
                    .settings
                    .ftp_servers
                    .iter_mut()
                    .find(|server| server.label() == prompt.server_label)
                {
                    server.saved_password = Some(sealed);
                }
                self.persist_settings("Password saved on this device.");
            }
            DialogState::Decompress(decompress) => {
                let request = decompress.selected_request();
                let archive_path = PathBuf::from(&request.archive.virtual_path);
                if crate::archive::check_archive_is_encrypted(&archive_path) {
                    self.pending_decompress_request = Some(request);
                    self.begin_text_input(TextInputPurpose::ArchivePassword, String::new());
                } else {
                    self.start_archive_decompression(request, None);
                }
            }
            DialogState::SortBy(sort_by) => {
                self.apply_active_pane_sort(sort_by.selected_sort_order());
            }
            DialogState::UpdateAvailable(prompt) => {
                if prompt.focused_index == 1 {
                    self.dialog = DialogState::None;
                    return DispatchResult::default();
                }
                let update = prompt.update.clone();
                let task_update = update.clone();
                let task_id = self.task_manager.spawn_threaded(
                    TaskKind::Download,
                    format!("Updating to version {}", update.version),
                    true,
                    move |reporter| update::install(reporter, task_update),
                );
                self.update_task = Some((task_id, update));
                self.dialog = DialogState::OperationProgress(task_id);
            }
            DialogState::UpdateInstalled(prompt) => {
                let target = prompt.update.target.clone();
                let restart_now = prompt.focused_index == 0;
                self.dialog = DialogState::None;
                if !restart_now {
                    self.status =
                        "The update takes effect the next time PadCommander starts.".to_owned();
                    return DispatchResult::default();
                }
                if let Err(error) = update::restart(&target) {
                    self.status = format!("Could not restart: {error}");
                    return DispatchResult::default();
                }
                return DispatchResult {
                    quit_requested: true,
                };
            }
            DialogState::Message { .. } => {
                self.dialog = DialogState::None;
                self.status = "Confirmed placeholder dialog.".to_owned();
            }
            DialogState::OperationProgress(task_id) => {
                let task_id = *task_id;
                let done = self
                    .task_manager
                    .task_by_id(task_id)
                    .is_none_or(|t| t.status.is_finished());
                if done {
                    self.dialog = DialogState::None;
                }
            }
            DialogState::None => self.open_or_confirm_selected(),
        }

        DispatchResult::default()
    }

    fn apply_text_submission(&mut self, purpose: TextInputPurpose, value: String) {
        let side = self.active_pane;
        let location = self.pane(side).current_location.clone();
        let edits_pane = matches!(
            purpose,
            TextInputPurpose::CreateFolder
                | TextInputPurpose::CreateFile
                | TextInputPurpose::CreateTextFile
                | TextInputPurpose::Rename
        );
        if edits_pane && ftp::is_ftp_location(&location) {
            let Some(connection) = self
                .ftp_sessions
                .get(&side)
                .map(|session| Arc::clone(&session.connection))
            else {
                self.status = "This pane is not connected to a server.".to_owned();
                return;
            };
            let selected = self
                .active_pane()
                .selected_markable_entry()
                .map(|entry| entry.virtual_path.clone());
            let events = self.ftp_events_tx.clone();
            self.status = "Working on the server...".to_owned();
            thread::spawn(move || {
                let target = ftp::child_location(&location, &value);
                let result = match (purpose, selected) {
                    (TextInputPurpose::Rename, Some(source)) => connection
                        .run(|stream| ftp::rename(stream, &source, &value))
                        .map(|()| format!("Renamed to {value}.")),
                    (TextInputPurpose::Rename, None) => {
                        Err("No item selected to rename.".to_owned())
                    }
                    (TextInputPurpose::CreateFolder, _) => connection
                        .run(|stream| ftp::create_folder(stream, &target))
                        .map(|()| format!("Created folder {value}.")),
                    _ => connection
                        .run(|stream| ftp::create_empty_file(stream, &target))
                        .map(|()| format!("Created file {value}.")),
                };
                let _ = events.send(FtpEvent::OperationDone { side, result });
            });
            return;
        }

        let result = match purpose {
            TextInputPurpose::CreateFolder => self
                .local_file_system
                .create_folder(&location, &value)
                .map(|path| format!("Created folder {}.", path.display())),
            TextInputPurpose::CreateFile => self
                .local_file_system
                .create_empty_file(&location, &value)
                .map(|path| format!("Created file {}.", path.display())),
            TextInputPurpose::CreateTextFile => self
                .local_file_system
                .create_empty_file(&location, &value)
                .map(|path| {
                    format!(
                        "Created empty text file {}. TODO: add multiline initial text input.",
                        path.display()
                    )
                }),
            TextInputPurpose::Rename => {
                let Some(source) = self
                    .active_pane()
                    .selected_markable_entry()
                    .map(|entry| entry.virtual_path.clone())
                else {
                    self.status = "No local item selected to rename.".to_owned();
                    return;
                };
                self.local_file_system
                    .rename_entry(&source, &value)
                    .map(|path| format!("Renamed item to {}.", path.display()))
            }
            TextInputPurpose::ConnectFtp => {
                let server = match ftp::parse_address(&value) {
                    Ok(server) => server,
                    Err(error) => {
                        self.status = error;
                        return;
                    }
                };
                if !self
                    .settings
                    .ftp_servers
                    .iter()
                    .any(|saved| saved.label() == server.label())
                {
                    self.settings.ftp_servers.push(server.clone());
                    self.persist_settings("Server saved.");
                }
                if server.is_anonymous() {
                    self.connect_ftp(server, String::new(), false);
                } else {
                    self.pending_ftp_server = Some(server);
                    self.begin_text_input(TextInputPurpose::FtpPassword, String::new());
                }
                return;
            }
            TextInputPurpose::FtpPassword => {
                let Some(server) = self.pending_ftp_server.take() else {
                    self.status = "No server is waiting for a password.".to_owned();
                    return;
                };
                self.connect_ftp(server, value, true);
                return;
            }
            TextInputPurpose::ArchiveName => {
                self.start_archive_compression(value);
                return;
            }
            TextInputPurpose::ArchiveDestination => {
                self.status = "Use Decompress to choose an extraction destination.".to_owned();
                return;
            }
            TextInputPurpose::ArchivePassword => {
                let Some(request) = self.pending_decompress_request.take() else {
                    self.status = "No pending archive extraction.".to_owned();
                    return;
                };
                self.start_archive_decompression(request, Some(value));
                return;
            }
            TextInputPurpose::ShortcutName => {
                self.submit_shortcut_name(value);
                return;
            }
        };

        match result {
            Ok(message) => {
                self.refresh_pane(side);
                self.status = message;
            }
            Err(error) => {
                self.status = error.user_message();
            }
        }
    }

    fn begin_delete_confirmation(&mut self) {
        let items = self.active_operation_sources();
        if items.is_empty() {
            self.status = "Delete: no available source items.".to_owned();
            return;
        }

        self.dialog = DialogState::DeleteConfirm(DeleteConfirmationState {
            pane: self.active_pane,
            items,
            focused_index: 0,
        });
        self.status = "Delete confirmation opened.".to_owned();
    }

    fn start_delete_task(&mut self, confirm: DeleteConfirmationState) {
        if let Some(session) = self.ftp_sessions.get(&confirm.pane) {
            let sources = ftp::remote_sources(&confirm.items);
            let server = session.connection.server.clone();
            let password = session.connection.password.clone();
            let label = format!("Deleting {} item(s)", sources.len());
            let task_id =
                self.task_manager
                    .spawn_threaded(TaskKind::Delete, label, true, move |reporter| {
                        ftp::delete(reporter, server, password, sources)
                            .unwrap_or_else(TaskOutcome::Failed)
                    });
            self.transfer_task_panes.insert(task_id, vec![confirm.pane]);
            self.dialog = DialogState::OperationProgress(task_id);
            return;
        }

        let paths: Vec<PathBuf> = confirm
            .items
            .iter()
            .map(|entry| PathBuf::from(&entry.virtual_path))
            .collect();
        let mode = if self.settings.use_recycle_bin {
            DeleteMode::RecycleBin
        } else {
            DeleteMode::Permanent
        };
        let label = format!("Deleting {} item(s)", paths.len());
        let task_id = self.task_manager.spawn_delete(label.clone(), paths, mode);
        self.delete_tasks.insert(
            task_id,
            DeleteTaskContext {
                pane: confirm.pane,
                mode,
            },
        );
        self.dialog = DialogState::OperationProgress(task_id);
    }

    fn begin_create_shortcut_in_other_pane(&mut self) {
        let Some(request) = self.create_pending_shortcut_request() else {
            return;
        };

        let default_name = request.default_shortcut_name();
        self.pending_shortcut_request = Some(request);
        self.begin_text_input(TextInputPurpose::ShortcutName, default_name);
    }

    fn create_pending_shortcut_request(&mut self) -> Option<PendingShortcutRequest> {
        let target_side = self.inactive_pane_side();
        let Some(entry) = self.active_pane().selected_markable_entry().cloned() else {
            self.status = "Shortcut: select an EXE file first.".to_owned();
            return None;
        };

        if !entry.is_exe_file() {
            self.status = format!("{} is not an EXE file.", entry.display_name);
            return None;
        }

        let target_location = self.pane(target_side).current_location.clone();
        if target_location == fs::ROOTS_LOCATION {
            self.status =
                "Shortcut: choose a destination folder in the other pane first.".to_owned();
            return None;
        }

        let target_path = PathBuf::from(&entry.virtual_path);
        let working_directory = target_path.parent().map(Path::to_path_buf);

        Some(PendingShortcutRequest {
            target_path: target_path.clone(),
            target_directory: PathBuf::from(target_location),
            working_directory,
            icon_path: Some(target_path),
            target_side,
        })
    }

    fn submit_shortcut_name(&mut self, value: String) {
        let Some(pending) = self.pending_shortcut_request.take() else {
            self.status = "Shortcut request is no longer active.".to_owned();
            return;
        };

        let request = pending.into_shortcut_request(shortcut_name_with_default_extension(&value));
        if request.shortcut_path.exists() {
            self.dialog = DialogState::ShortcutConflict(ShortcutConflictState {
                request,
                focused_index: 0,
            });
            self.status = "Shortcut already exists.".to_owned();
        } else {
            self.start_create_shortcut(request, false);
        }
    }

    fn clear_pending_text_input_state(&mut self) {
        if matches!(
            self.dialog,
            DialogState::TextInput(TextInputDialogState {
                purpose: TextInputPurpose::ShortcutName,
                ..
            })
        ) {
            self.pending_shortcut_request = None;
        }
        if matches!(
            self.dialog,
            DialogState::TextInput(TextInputDialogState {
                purpose: TextInputPurpose::ArchivePassword,
                ..
            })
        ) {
            self.pending_decompress_request = None;
        }
        if matches!(
            self.dialog,
            DialogState::TextInput(TextInputDialogState {
                purpose: TextInputPurpose::FtpPassword,
                ..
            })
        ) {
            self.pending_ftp_server = None;
        }
    }

    fn start_create_shortcut(&mut self, request: ShortcutRequest, overwrite: bool) {
        if request.shortcut_path.exists() && !overwrite {
            self.dialog = DialogState::ShortcutConflict(ShortcutConflictState {
                request,
                focused_index: 0,
            });
            self.status = "Shortcut already exists.".to_owned();
            return;
        }

        let result = platform::create_shortcut(
            &request.target_path,
            &request.shortcut_path,
            request.working_directory.as_deref(),
            request.icon_path.as_deref(),
        );

        self.dialog = DialogState::None;
        match result {
            Ok(()) => {
                self.refresh_pane(request.target_side);
                self.status = format!("Created shortcut {}.", request.shortcut_path.display());
            }
            Err(error) => {
                self.status = format!(
                    "Cannot create shortcut {}: {error}",
                    request.shortcut_path.display()
                );
            }
        }
    }

    fn start_transfer_task(&mut self, kind: TaskKind) {
        let source_side = self.active_pane;
        let target_side = self.inactive_pane_side();
        let sources = self.active_operation_sources();
        if sources.is_empty() {
            self.status = format!("{}: no available source items.", kind.label());
            return;
        }

        let target_location = self.pane(target_side).current_location.clone();
        if target_location == fs::ROOTS_LOCATION {
            self.status = format!(
                "{}: choose a target folder in the other pane first.",
                kind.label()
            );
            return;
        }

        let target_is_ftp = ftp::is_ftp_location(&target_location);
        if target_is_ftp && ftp::is_ftp_location(&self.pane(source_side).current_location) {
            self.status = format!(
                "{}: transfers between two servers are not supported.",
                kind.label()
            );
            return;
        }

        let label = format!(
            "{} {} item(s) to {}",
            kind.label(),
            sources.len(),
            fs::location_label(&target_location)
        );

        // A server pane's listing is the cheapest way to see what already exists there.
        let target_entries = &self.pane(target_side).entries;
        let conflicts: Vec<String> = sources
            .iter()
            .map(|entry| entry.display_name.clone())
            .filter(|name| {
                if target_is_ftp {
                    target_entries
                        .iter()
                        .any(|entry| &entry.display_name == name)
                } else {
                    Path::new(&target_location).join(name).exists()
                }
            })
            .collect();

        if !conflicts.is_empty() {
            self.dialog = DialogState::TransferConflict(TransferConflictState {
                kind,
                source_side,
                target_side,
                sources,
                target_location,
                conflicts,
                label,
                focused_index: 0,
            });
            return;
        }

        self.execute_transfer_task(
            kind,
            source_side,
            target_side,
            sources,
            target_location,
            label,
            false,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_transfer_task(
        &mut self,
        kind: TaskKind,
        source_side: PaneSide,
        target_side: PaneSide,
        sources: Vec<FileEntry>,
        target_location: String,
        label: String,
        overwrite: bool,
    ) {
        let delete_sources = kind == TaskKind::Move;
        let source_connection = self
            .ftp_sessions
            .get(&source_side)
            .map(|s| Arc::clone(&s.connection));
        let target_connection = self
            .ftp_sessions
            .get(&target_side)
            .map(|s| Arc::clone(&s.connection));
        let source_paths: Vec<PathBuf> = sources
            .iter()
            .map(|entry| PathBuf::from(&entry.virtual_path))
            .collect();
        let target_directory = PathBuf::from(&target_location);

        let task_id = if let Some(connection) = source_connection {
            let remote_sources = ftp::remote_sources(&sources);
            let server = connection.server.clone();
            let password = connection.password.clone();
            self.task_manager
                .spawn_threaded(TaskKind::Download, label, true, move |reporter| {
                    ftp::download(
                        reporter,
                        server,
                        password,
                        remote_sources,
                        target_directory,
                        delete_sources,
                    )
                    .unwrap_or_else(TaskOutcome::Failed)
                })
        } else if let Some(connection) = target_connection {
            let server = connection.server.clone();
            let password = connection.password.clone();
            self.task_manager
                .spawn_threaded(TaskKind::Upload, label, true, move |reporter| {
                    ftp::upload(
                        reporter,
                        server,
                        password,
                        source_paths,
                        target_location,
                        delete_sources,
                    )
                    .unwrap_or_else(TaskOutcome::Failed)
                })
        } else {
            match kind {
                TaskKind::Copy if overwrite => self.task_manager.spawn_copy_to_directory_overwrite(
                    label.clone(),
                    source_paths,
                    target_directory,
                ),
                TaskKind::Copy => self.task_manager.spawn_copy_to_directory(
                    label.clone(),
                    source_paths,
                    target_directory,
                ),
                TaskKind::Move if overwrite => self.task_manager.spawn_move_to_directory_overwrite(
                    label.clone(),
                    source_paths,
                    target_directory,
                ),
                TaskKind::Move => self.task_manager.spawn_move_to_directory(
                    label.clone(),
                    source_paths,
                    target_directory,
                ),
                _ => {
                    self.status = format!("{} is not a local transfer action.", kind.label());
                    return;
                }
            }
        };

        let refresh_sides = if kind == TaskKind::Move {
            vec![source_side, target_side]
        } else {
            vec![target_side]
        };
        self.transfer_task_panes.insert(task_id, refresh_sides);
        self.dialog = DialogState::OperationProgress(task_id);
    }

    fn begin_archive_name_input(&mut self) {
        if self.active_operation_sources().is_empty() {
            self.status = "Compress: no available source items.".to_owned();
            return;
        }

        let target_side = self.inactive_pane_side();
        let target_location = self.pane(target_side).current_location.clone();
        if target_location == fs::ROOTS_LOCATION {
            self.status = "Compress: choose a target folder in the other pane first.".to_owned();
            return;
        }

        self.begin_text_input(TextInputPurpose::ArchiveName, "archive.zip".to_owned());
    }

    fn begin_archive_decompression_choices(&mut self) {
        let Some(archive) = self.active_pane().selected_markable_entry().cloned() else {
            self.status = "Decompress: select an archive first.".to_owned();
            return;
        };

        if archive.kind != FileEntryKind::Archive {
            self.status = format!("{} is not a supported archive.", archive.display_name);
            return;
        }

        if !archive_name_is_supported(&archive.display_name) {
            self.status =
                "Unsupported archive type. Supported types are .zip, .rar, and .7z.".to_owned();
            return;
        }

        let archive_stem = PathBuf::from(&archive.display_name)
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .filter(|stem| !stem.is_empty())
            .unwrap_or_else(|| "extracted".to_owned());

        self.dialog = DialogState::Decompress(DecompressModalState {
            focused_index: 0,
            archive,
            archive_stem,
            source_side: self.active_pane,
        });
        self.status = "Choose decompression destination.".to_owned();
    }

    fn start_archive_compression(&mut self, archive_name: String) {
        let source_side = self.active_pane;
        let target_side = self.inactive_pane_side();
        let sources = self.active_operation_sources();
        if sources.is_empty() {
            self.status = "Compress: no available source items.".to_owned();
            return;
        }

        let target_location = self.pane(target_side).current_location.clone();
        if target_location == fs::ROOTS_LOCATION {
            self.status = "Compress: choose a target folder in the other pane first.".to_owned();
            return;
        }

        let archive_name = archive_name_with_default_extension(&archive_name);
        if !archive_name_is_supported(&archive_name) {
            self.status =
                "Unsupported archive type. Supported types are .zip, .rar, and .7z.".to_owned();
            return;
        }

        let source_paths = sources
            .iter()
            .map(|entry| PathBuf::from(&entry.virtual_path))
            .collect::<Vec<_>>();
        let archive_path = PathBuf::from(&target_location).join(&archive_name);
        let label = format!(
            "Compress {} item(s) to {}",
            source_paths.len(),
            archive_path.display()
        );
        let task_id =
            self.task_manager
                .spawn_compress_to_archive(label.clone(), source_paths, archive_path);
        self.transfer_task_panes.insert(task_id, vec![target_side]);
        self.dialog = DialogState::OperationProgress(task_id);

        let pane = self.pane_mut(source_side);
        pane.marked_items.clear();
    }

    fn start_archive_decompression(
        &mut self,
        request: DecompressRequest,
        password: Option<String>,
    ) {
        let target_location = self.pane(request.target_side).current_location.clone();
        if target_location == fs::ROOTS_LOCATION {
            self.status = format!(
                "Decompress: choose a target folder in the {} pane first.",
                request.target_side.label().to_ascii_lowercase()
            );
            return;
        }

        let archive_path = PathBuf::from(&request.archive.virtual_path);
        let destination = if request.create_archive_folder {
            PathBuf::from(&target_location).join(&request.archive_stem)
        } else {
            PathBuf::from(&target_location)
        };
        let label = format!(
            "Decompress {} to {}",
            request.archive.display_name,
            destination.display()
        );
        let task_id = self.task_manager.spawn_decompress_archive(
            label.clone(),
            archive_path,
            destination,
            password,
        );
        self.transfer_task_panes
            .insert(task_id, vec![request.target_side]);
        self.dialog = DialogState::OperationProgress(task_id);
    }

    fn start_demo_task(&mut self, failing: bool) {
        let (kind, title, steps, fail_at_step) = if failing {
            (
                TaskKind::Download,
                "Failing demo download".to_owned(),
                20,
                Some(12),
            )
        } else {
            (TaskKind::Copy, "Demo copy task".to_owned(), 40, None)
        };
        let task_id = self
            .task_manager
            .spawn_synthetic_task(kind, title, steps, fail_at_step);
        self.status = format!("Started task #{task_id}.");
    }

    fn cancel_active_task(&mut self) -> bool {
        if let Some(task_id) = self.task_manager.cancel_active_task() {
            self.status = format!("Cancellation requested for task #{task_id}.");
            true
        } else {
            false
        }
    }

    fn navigate_active_pane(&mut self, location: String) {
        let active_pane = self.active_pane;
        let previous_location = self.pane(active_pane).current_location.clone();
        self.push_navigation_history(active_pane, &location);
        self.pane_mut(active_pane).current_location = location;
        self.remember_pane_location(active_pane);
        self.refresh_pane(active_pane);

        if matches!(self.pane(active_pane).load_state, PaneLoadState::Error(_)) {
            let error_status = self.status.clone();
            self.pane_mut(active_pane).navigation_history.pop();
            self.pane_mut(active_pane).current_location = previous_location.clone();
            self.remember_pane_location(active_pane);
            self.refresh_pane(active_pane);
            self.status = error_status;
        }
    }

    fn go_back(&mut self) {
        if self.show_input_help {
            self.show_input_help = false;
            self.status = "Input help closed.".to_owned();
        } else if self.text_dialog_is_open() {
            self.handle_text_input(TextInputAction::Cancel);
        } else if self.dialog != DialogState::None {
            self.dialog = DialogState::None;
            self.status = "Cancelled.".to_owned();
        } else if self.clear_active_error() {
            self.status = "Error dismissed.".to_owned();
        } else {
            self.go_back_or_parent(self.active_pane);
        }
    }

    fn go_back_or_parent(&mut self, side: PaneSide) {
        while let Some(snapshot) = self.pane_mut(side).navigation_history.pop() {
            if self.location_exists(side, &snapshot.location) {
                self.pane_mut(side).current_location = snapshot.location.clone();
                self.remember_pane_location(side);
                self.refresh_pane(side);
                self.pane_mut(side).restore_snapshot_position(snapshot);
                return;
            }

            let fallback_location = self.parent_of(side, &snapshot.location);
            if self.location_exists(side, &fallback_location) {
                self.pane_mut(side).current_location = fallback_location;
                self.remember_pane_location(side);
                self.refresh_pane(side);
                return;
            }
        }

        self.go_parent(side);
    }

    fn go_parent(&mut self, side: PaneSide) {
        let parent = self.parent_of(side, &self.pane(side).current_location);
        self.pane_mut(side).current_location = parent;
        self.remember_pane_location(side);
        self.refresh_pane(side);
    }

    fn refresh_pane(&mut self, side: PaneSide) {
        let location = self.pane(side).current_location.clone();
        if !ftp::is_ftp_location(&location) {
            self.ftp_sessions.remove(&side);
            let result = self
                .local_file_system
                .list_location(&location)
                .map_err(|error| error.user_message());
            self.pane_mut(side).cached_free_space = fs::get_free_space(&location);
            self.apply_listing(side, &location, result);
            return;
        }

        let Some(connection) = self
            .ftp_sessions
            .get(&side)
            .map(|session| Arc::clone(&session.connection))
        else {
            let message = "Not connected. Press Ctrl+F or R3 to open Servers.".to_owned();
            self.apply_listing(side, &location, Err(message));
            return;
        };
        let parent_location = self.parent_of(side, &location);
        let events = self.ftp_events_tx.clone();
        let pane = self.pane_mut(side);
        pane.load_state = PaneLoadState::Loading;
        pane.cached_free_space = None;
        self.status = format!("Loading {location}...");
        thread::spawn(move || {
            let result = connection.run(|stream| ftp::list(stream, &location, &parent_location));
            let _ = events.send(FtpEvent::Listed {
                side,
                location,
                result,
            });
        });
    }

    fn apply_listing(
        &mut self,
        side: PaneSide,
        location: &str,
        result: Result<Vec<FileEntry>, String>,
    ) {
        let previous_selection_path = self
            .pane(side)
            .selected_entry()
            .map(|entry| entry.virtual_path.clone());
        let show_hidden_files = self.settings.show_hidden_files;
        let pane = self.pane_mut(side);

        match result {
            Ok(entries) => {
                pane.entries = filter_hidden_entries(entries, show_hidden_files);
                pane.sort_entries();
                pane.retain_valid_marks();
                pane.restore_selection(previous_selection_path.as_deref());
                pane.load_state = PaneLoadState::Ready;
                self.status = format!("Loaded {}.", fs::location_label(location));
            }
            Err(message) => {
                pane.entries.clear();
                pane.reset_selection();
                pane.marked_items.clear();
                pane.load_state = PaneLoadState::Error(message.clone());
                self.status = message;
            }
        }
    }

    /// Parent of a pane location; the root of a server leads back to where the pane was before connecting.
    fn parent_of(&self, side: PaneSide, location: &str) -> String {
        if !ftp::is_ftp_location(location) {
            return fs::parent_location(location);
        }

        ftp::parent_location(location)
            .or_else(|| {
                self.ftp_sessions
                    .get(&side)
                    .map(|session| session.return_location.clone())
            })
            .unwrap_or_else(|| fs::ROOTS_LOCATION.to_owned())
    }

    fn location_exists(&self, side: PaneSide, location: &str) -> bool {
        if !ftp::is_ftp_location(location) {
            return fs::location_exists(location);
        }

        self.ftp_sessions
            .get(&side)
            .is_some_and(|session| location.starts_with(&session.connection.server.root_location()))
    }

    fn clear_active_error(&mut self) -> bool {
        let pane = self.pane_mut(self.active_pane);
        if matches!(pane.load_state, PaneLoadState::Error(_)) {
            pane.load_state = PaneLoadState::Ready;
            true
        } else {
            false
        }
    }

    fn move_command_menu_focus(&mut self, movement: SelectionMovement) -> bool {
        match &mut self.dialog {
            DialogState::CommandMenu(menu) => {
                menu.move_focus(movement);
                true
            }
            DialogState::Settings(settings) => {
                settings.move_focus(movement);
                true
            }
            DialogState::Favourites(favourites) => {
                favourites.move_focus(movement, self.settings.favourites.len());
                true
            }
            DialogState::FtpServers(servers) => {
                // The extra last row is "Add server".
                servers.move_focus(movement, self.settings.ftp_servers.len() + 1);
                true
            }
            DialogState::SavePassword(prompt) => {
                prompt.move_focus(movement);
                true
            }
            DialogState::Decompress(decompress) => {
                decompress.move_focus(movement);
                true
            }
            DialogState::SortBy(sort_by) => {
                sort_by.move_focus(movement);
                true
            }
            DialogState::DeleteConfirm(confirm) => {
                confirm.move_focus(movement);
                true
            }
            DialogState::ShortcutConflict(conflict) => {
                conflict.move_focus(movement);
                true
            }
            DialogState::TransferConflict(conflict) => {
                conflict.move_focus(movement);
                true
            }
            DialogState::UpdateAvailable(prompt) | DialogState::UpdateInstalled(prompt) => {
                prompt.move_focus(movement);
                true
            }
            DialogState::None
            | DialogState::Message { .. }
            | DialogState::TextInput(_)
            | DialogState::OperationProgress(_) => false,
        }
    }

    fn apply_active_pane_sort(&mut self, sort_order: PaneSortOrder) {
        let active_pane = self.active_pane;
        let pane_label = self.active_pane_label();
        let previous_selection_path = self
            .pane(active_pane)
            .selected_entry()
            .map(|entry| entry.virtual_path.clone());
        let pane = self.pane_mut(active_pane);
        pane.sort_order = sort_order;
        pane.sort_entries();
        pane.restore_selection(previous_selection_path.as_deref());
        self.dialog = DialogState::None;
        self.status = format!("{pane_label} pane sorted by {}.", sort_order.label());
    }

    fn adjust_focused_setting(&mut self, delta: f32) {
        if let DialogState::Settings(settings) = &self.dialog {
            match settings.focused_setting() {
                SettingsModalFocus::UiScale => self.adjust_ui_scale(delta),
                SettingsModalFocus::UseRecycleBin => self.toggle_use_recycle_bin(),
                SettingsModalFocus::ShowHiddenFiles => self.toggle_show_hidden_files(),
                SettingsModalFocus::ControllerOverride => {
                    self.cycle_controller_override(delta > 0.0)
                }
                SettingsModalFocus::Theme => self.cycle_theme(delta > 0.0),
                SettingsModalFocus::CheckForUpdates => {
                    self.status = "Press A / Enter to check for updates.".to_owned();
                }
                SettingsModalFocus::Quit => {
                    self.status = "Press A / Enter to quit.".to_owned();
                }
            }
        }
    }

    fn activate_focused_setting(&mut self) -> DispatchResult {
        let Some(focused_setting) = self.focused_setting() else {
            return DispatchResult::default();
        };

        match focused_setting {
            SettingsModalFocus::UiScale => {
                self.status = "Use Left / Right to change UI scale.".to_owned();
            }
            SettingsModalFocus::UseRecycleBin => self.toggle_use_recycle_bin(),
            SettingsModalFocus::ShowHiddenFiles => self.toggle_show_hidden_files(),
            SettingsModalFocus::ControllerOverride => self.cycle_controller_override(true),
            SettingsModalFocus::Theme => self.cycle_theme(true),
            SettingsModalFocus::CheckForUpdates => {
                return self.dispatch_action(CommanderAction::CheckForUpdates);
            }
            SettingsModalFocus::Quit => {
                self.status = "Quit requested.".to_owned();
                return DispatchResult {
                    quit_requested: true,
                };
            }
        }

        DispatchResult::default()
    }

    fn focused_setting(&self) -> Option<SettingsModalFocus> {
        if let DialogState::Settings(settings) = &self.dialog {
            Some(settings.focused_setting())
        } else {
            None
        }
    }

    fn adjust_ui_scale(&mut self, delta: f32) {
        self.settings.ui_scale = settings::clamp_ui_scale(self.settings.ui_scale + delta);
        self.persist_settings("UI scale updated.");
    }

    fn toggle_use_recycle_bin(&mut self) {
        self.settings.use_recycle_bin = !self.settings.use_recycle_bin;
        let status = if self.settings.use_recycle_bin {
            "Delete will use the Recycle Bin."
        } else {
            "Delete will permanently remove files."
        };
        self.persist_settings(status);
    }

    fn cycle_controller_override(&mut self, forward: bool) {
        let current = self.settings.controller_override;
        self.settings.controller_override = if forward {
            current.next()
        } else {
            current.prev()
        };
        self.persist_settings(&format!(
            "Controller buttons: {}.",
            self.settings.controller_override.label()
        ));
    }

    fn cycle_theme(&mut self, forward: bool) {
        let current = self.settings.theme;
        self.settings.theme = if forward {
            current.next()
        } else {
            current.prev()
        };
        self.persist_settings(&format!("Theme: {}.", self.settings.theme.label()));
    }

    fn toggle_show_hidden_files(&mut self) {
        self.settings.show_hidden_files = !self.settings.show_hidden_files;
        let status = if self.settings.show_hidden_files {
            "Hidden files are visible."
        } else {
            "Hidden files are hidden."
        };
        self.refresh_pane(PaneSide::Left);
        self.refresh_pane(PaneSide::Right);
        self.persist_settings(status);
    }

    fn add_active_location_to_favourites(&mut self) {
        let location = self.active_pane().current_location.clone();
        if location == fs::ROOTS_LOCATION {
            self.status = "Choose a folder before adding a favourite.".to_owned();
            return;
        }
        if ftp::is_ftp_location(&location) {
            self.status = "Servers are saved in Servers, not Favourites.".to_owned();
            return;
        }

        if !self.settings.favourites.contains(&location) {
            self.settings.favourites.push(location.clone());
            self.settings.favourites.sort();
        }

        if let DialogState::Favourites(favourites) = &mut self.dialog {
            favourites.focused_index = self
                .settings
                .favourites
                .iter()
                .position(|favourite| favourite == &location)
                .unwrap_or(0);
        }

        self.persist_settings("Favourite saved.");
    }

    fn remove_focused_favourite(&mut self) {
        let Some(index) = self.focused_favourite_index() else {
            self.status = "No favourite selected.".to_owned();
            return;
        };

        let removed = self.settings.favourites.remove(index);
        if let DialogState::Favourites(favourites) = &mut self.dialog {
            favourites.clamp_focus(self.settings.favourites.len());
        }
        self.persist_settings(&format!(
            "Removed favourite {}.",
            fs::location_label(&removed)
        ));
    }

    fn open_focused_favourite(&mut self) {
        let Some(index) = self.focused_favourite_index() else {
            self.status = "No favourite selected.".to_owned();
            return;
        };
        let location = self.settings.favourites[index].clone();

        match self.local_file_system.list_location(&location) {
            Ok(_) => {
                self.dialog = DialogState::None;
                let active_pane = self.active_pane;
                self.push_navigation_history(active_pane, &location);
                self.pane_mut(active_pane).current_location = location.clone();
                self.remember_pane_location(active_pane);
                self.refresh_pane(active_pane);
                self.status = format!("Opened favourite {}.", fs::location_label(&location));
            }
            Err(error) => {
                self.status = format!(
                    "Cannot open favourite {}: {}",
                    fs::location_label(&location),
                    error.user_message()
                );
            }
        }
    }

    fn focused_favourite_index(&self) -> Option<usize> {
        let DialogState::Favourites(favourites) = &self.dialog else {
            return None;
        };

        if self.settings.favourites.is_empty() {
            None
        } else {
            Some(
                favourites
                    .focused_index
                    .min(self.settings.favourites.len() - 1),
            )
        }
    }

    fn focus_favourite(&mut self, index: usize) {
        let DialogState::Favourites(favourites) = &mut self.dialog else {
            return;
        };

        if self.settings.favourites.is_empty() {
            favourites.focused_index = 0;
        } else {
            favourites.focused_index = index.min(self.settings.favourites.len() - 1);
        }
    }

    fn focus_decompress_option(&mut self, index: usize) {
        let DialogState::Decompress(decompress) = &mut self.dialog else {
            return;
        };

        decompress.focused_index = index.min(DecompressChoice::COUNT - 1);
    }

    fn remember_pane_location(&mut self, side: PaneSide) {
        let location = self.pane(side).current_location.clone();
        // Server sessions are not restored on launch, so only local folders are remembered.
        if ftp::is_ftp_location(&location) {
            return;
        }
        match side {
            PaneSide::Left => self.settings.last_left_pane_location = Some(location),
            PaneSide::Right => self.settings.last_right_pane_location = Some(location),
        }
        self.persist_settings("Pane location saved.");
    }

    fn push_navigation_history(&mut self, side: PaneSide, next_location: &str) {
        if self.pane(side).current_location == next_location {
            return;
        }

        let snapshot = self.pane(side).navigation_snapshot();
        self.pane_mut(side).navigation_history.push(snapshot);
    }

    fn persist_settings(&mut self, success_status: &str) {
        match settings::save_to_path(&self.settings, &self.settings_path) {
            Ok(()) => self.status = success_status.to_owned(),
            Err(error) => {
                self.status = format!(
                    "Could not save settings to {}: {error}",
                    self.settings_path.display()
                );
            }
        }
    }

    pub fn active_operation_sources(&self) -> Vec<FileEntry> {
        self.active_pane().operation_sources()
    }

    pub fn text_dialog_is_open(&self) -> bool {
        matches!(self.dialog, DialogState::TextInput(_))
    }

    pub fn settings_dialog_is_open(&self) -> bool {
        matches!(self.dialog, DialogState::Settings(_))
    }

    pub fn favourites_dialog_is_open(&self) -> bool {
        matches!(self.dialog, DialogState::Favourites(_))
    }

    pub fn modal_is_active(&self) -> bool {
        !matches!(
            self.dialog,
            DialogState::None | DialogState::TextInput(_) | DialogState::Settings(_)
        )
    }

    pub fn can_create_shortcut_in_other_pane(&self) -> bool {
        platform::SUPPORTS_SHORTCUTS
            && self.ftp_sessions.is_empty()
            && self
                .active_pane()
                .selected_markable_entry()
                .map(FileEntry::is_exe_file)
                .unwrap_or(false)
    }

    pub fn ftp_servers_dialog_is_open(&self) -> bool {
        matches!(self.dialog, DialogState::FtpServers(_))
    }

    pub fn active_pane_is_on_server(&self) -> bool {
        self.ftp_sessions.contains_key(&self.active_pane)
    }

    fn config_dir(&self) -> PathBuf {
        self.settings_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(platform::config_dir)
    }

    fn connect_ftp(&mut self, server: FtpServer, password: String, offer_save_password: bool) {
        let side = self.active_pane;
        let current_location = self.pane(side).current_location.clone();
        let return_location = match self.ftp_sessions.get(&side) {
            Some(session) => session.return_location.clone(),
            None => current_location,
        };
        let location = server.start_location();
        self.ftp_sessions.insert(
            side,
            FtpSession {
                connection: Arc::new(FtpConnection::new(server, password)),
                return_location,
                offer_save_password,
            },
        );
        self.navigate_active_pane(location);
    }

    fn remove_focused_ftp_server(&mut self) {
        let DialogState::FtpServers(servers) = &mut self.dialog else {
            return;
        };
        if servers.focused_index >= self.settings.ftp_servers.len() {
            self.status = "No server selected.".to_owned();
            return;
        }

        let removed = self.settings.ftp_servers.remove(servers.focused_index);
        servers.clamp_focus(self.settings.ftp_servers.len() + 1);
        self.persist_settings(&format!("Removed {}.", removed.label()));
    }

    fn text_dialog_mut(&mut self) -> Option<&mut TextInputDialogState> {
        match &mut self.dialog {
            DialogState::TextInput(dialog) => Some(dialog),
            DialogState::None
            | DialogState::Message { .. }
            | DialogState::DeleteConfirm(_)
            | DialogState::ShortcutConflict(_)
            | DialogState::TransferConflict(_)
            | DialogState::CommandMenu(_)
            | DialogState::Settings(_)
            | DialogState::Favourites(_)
            | DialogState::FtpServers(_)
            | DialogState::SavePassword(_)
            | DialogState::Decompress(_)
            | DialogState::SortBy(_)
            | DialogState::OperationProgress(_)
            | DialogState::UpdateAvailable(_)
            | DialogState::UpdateInstalled(_) => None,
        }
    }
}

fn archive_name_with_default_extension(value: &str) -> String {
    let trimmed = value.trim();
    if PathBuf::from(trimmed).extension().is_some() {
        trimmed.to_owned()
    } else {
        format!("{trimmed}.zip")
    }
}

fn shortcut_name_with_default_extension(value: &str) -> String {
    let trimmed = value.trim();
    if PathBuf::from(trimmed)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("lnk"))
        .unwrap_or(false)
    {
        trimmed.to_owned()
    } else {
        format!("{trimmed}.lnk")
    }
}

fn archive_name_is_supported(value: &str) -> bool {
    PathBuf::from(value)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "zip" | "rar" | "7z"
            )
        })
        .unwrap_or(false)
}

fn filter_hidden_entries(entries: Vec<FileEntry>, show_hidden_files: bool) -> Vec<FileEntry> {
    if show_hidden_files {
        return entries;
    }

    entries
        .into_iter()
        .filter(|entry| matches!(entry.kind, FileEntryKind::Parent) || !entry.flags.hidden)
        .collect()
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DispatchResult {
    pub quit_requested: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::CommanderAction;
    use std::{
        collections::BTreeSet,
        time::{Duration, SystemTime},
    };

    fn test_entry(name: &str) -> FileEntry {
        FileEntry {
            display_name: name.to_owned(),
            virtual_path: name.to_owned(),
            kind: FileEntryKind::File,
            size: Some(1024),
            modified: None,
            flags: FileEntryFlags::default(),
        }
    }

    fn hidden_test_entry(name: &str) -> FileEntry {
        FileEntry {
            flags: FileEntryFlags {
                hidden: true,
                read_only: false,
            },
            ..test_entry(name)
        }
    }

    fn directory_entry(name: &str) -> FileEntry {
        FileEntry {
            kind: FileEntryKind::Directory,
            size: None,
            ..test_entry(name)
        }
    }

    fn parent_entry() -> FileEntry {
        FileEntry {
            display_name: "..".to_owned(),
            virtual_path: fs::ROOTS_LOCATION.to_owned(),
            kind: FileEntryKind::Parent,
            size: None,
            modified: None,
            flags: FileEntryFlags::default(),
        }
    }

    fn test_state() -> AppState {
        let (ftp_events_tx, ftp_events_rx) = mpsc::channel();
        AppState {
            settings: Settings::default(),
            settings_path: std::env::temp_dir().join("pad_commander_test_settings.json"),
            left_pane: PaneState {
                current_location: fs::ROOTS_LOCATION.to_owned(),
                entries: vec![test_entry("a"), test_entry("b"), test_entry("c")],
                selected_index: 1,
                marked_items: BTreeSet::new(),
                load_state: PaneLoadState::Ready,
                sort_order: PaneSortOrder::default(),
                scroll_offset: 0.0,
                needs_scroll_to_selected: false,
                cached_free_space: None,
                navigation_history: Vec::new(),
            },
            right_pane: PaneState {
                current_location: fs::ROOTS_LOCATION.to_owned(),
                entries: vec![test_entry("x"), test_entry("y"), test_entry("z")],
                selected_index: 1,
                marked_items: BTreeSet::new(),
                load_state: PaneLoadState::Ready,
                sort_order: PaneSortOrder::default(),
                scroll_offset: 0.0,
                needs_scroll_to_selected: false,
                cached_free_space: None,
                navigation_history: Vec::new(),
            },
            active_pane: PaneSide::Left,
            status: String::new(),
            dialog: DialogState::None,
            show_input_help: false,
            local_file_system: LocalFileSystem,
            task_manager: TaskManager::default(),
            delete_tasks: BTreeMap::new(),
            transfer_task_panes: BTreeMap::new(),
            pending_shortcut_request: None,
            pending_decompress_request: None,
            pending_ftp_server: None,
            ftp_sessions: BTreeMap::new(),
            ftp_events_tx,
            ftp_events_rx,
            update_check: None,
            update_task: None,
        }
    }

    fn test_update() -> Update {
        Update {
            version: "99.0.0".to_owned(),
            asset_name: "app.zip".to_owned(),
            download_url: "https://example.test/app.zip".to_owned(),
            checksums_url: "https://example.test/SHA256SUMS".to_owned(),
            size: 1,
            target: PathBuf::from("pad-commander"),
        }
    }

    fn deliver_update_check(state: &mut AppState, result: Result<Option<Update>, String>) {
        let (sender, receiver) = mpsc::channel();
        sender.send(result).unwrap();
        state.update_check = Some(receiver);
        state.poll_tasks();
    }

    #[test]
    fn update_check_result_opens_matching_dialog() {
        let mut state = test_state();

        deliver_update_check(&mut state, Ok(Some(test_update())));
        assert!(matches!(
            &state.dialog,
            DialogState::UpdateAvailable(prompt) if prompt.update.version == "99.0.0"
        ));
        assert!(state.update_check.is_none());

        deliver_update_check(&mut state, Ok(None));
        assert!(
            matches!(&state.dialog, DialogState::Message { title, .. } if title == "Up to Date")
        );

        deliver_update_check(&mut state, Err("offline".to_owned()));
        assert!(matches!(
            &state.dialog,
            DialogState::Message { message, .. } if message == "offline"
        ));
    }

    #[test]
    fn declining_update_prompts_closes_them_without_quitting() {
        let mut state = test_state();
        for dialog in [
            DialogState::UpdateAvailable(UpdatePromptState {
                update: test_update(),
                focused_index: 0,
            }),
            DialogState::UpdateInstalled(UpdatePromptState {
                update: test_update(),
                focused_index: 0,
            }),
        ] {
            state.dialog = dialog;
            state.dispatch_action(CommanderAction::MoveSelectionDown);
            let result = state.dispatch_action(CommanderAction::Confirm);

            assert_eq!(state.dialog, DialogState::None);
            assert!(!result.quit_requested);
        }
        assert!(state.update_task.is_none());
    }

    #[test]
    fn switching_panes_toggles_active_side() {
        let mut state = test_state();

        assert_eq!(state.active_pane, PaneSide::Left);

        state.dispatch_action(CommanderAction::SwitchPane);
        assert_eq!(state.active_pane, PaneSide::Right);

        state.dispatch_action(CommanderAction::SwitchPane);
        assert_eq!(state.active_pane, PaneSide::Left);
    }

    #[test]
    fn command_menu_opens_sort_by_modal() {
        let mut state = test_state();

        state.dispatch_action(CommanderAction::OpenCommandPalette);
        let DialogState::CommandMenu(menu) = &mut state.dialog else {
            panic!("expected command menu");
        };
        menu.focused_index = menu
            .entries
            .iter()
            .position(|entry| entry.action == CommanderAction::OpenSortBy)
            .unwrap();

        state.dispatch_action(CommanderAction::Confirm);

        assert!(matches!(state.dialog, DialogState::SortBy(_)));
    }

    #[test]
    fn sort_by_filename_desc_reorders_current_pane_with_groups_preserved() {
        let mut state = test_state();
        state.left_pane.entries = vec![
            test_entry("alpha.txt"),
            directory_entry("beta"),
            parent_entry(),
            test_entry("zulu.txt"),
            directory_entry("alpha"),
        ];

        state.dispatch_action(CommanderAction::SelectSortOrder(
            PaneSortOrder::FilenameDesc,
        ));

        let names: Vec<_> = state
            .left_pane
            .entries
            .iter()
            .map(|entry| entry.display_name.as_str())
            .collect();
        assert_eq!(names, ["..", "beta", "alpha", "zulu.txt", "alpha.txt"]);
    }

    #[test]
    fn sort_by_date_and_size_keep_unknown_values_last() {
        let mut state = test_state();
        let older = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        let newer = SystemTime::UNIX_EPOCH + Duration::from_secs(20);
        state.left_pane.entries = vec![
            FileEntry {
                display_name: "unknown.txt".to_owned(),
                size: None,
                modified: None,
                ..test_entry("unknown.txt")
            },
            FileEntry {
                display_name: "old-small.txt".to_owned(),
                size: Some(1),
                modified: Some(older),
                ..test_entry("old-small.txt")
            },
            FileEntry {
                display_name: "new-large.txt".to_owned(),
                size: Some(10),
                modified: Some(newer),
                ..test_entry("new-large.txt")
            },
        ];

        state.dispatch_action(CommanderAction::SelectSortOrder(PaneSortOrder::DateDesc));
        let date_names: Vec<_> = state
            .left_pane
            .entries
            .iter()
            .map(|entry| entry.display_name.as_str())
            .collect();
        assert_eq!(
            date_names,
            ["new-large.txt", "old-small.txt", "unknown.txt"]
        );

        state.dispatch_action(CommanderAction::SelectSortOrder(PaneSortOrder::SizeAsc));
        let size_names: Vec<_> = state
            .left_pane
            .entries
            .iter()
            .map(|entry| entry.display_name.as_str())
            .collect();
        assert_eq!(
            size_names,
            ["old-small.txt", "new-large.txt", "unknown.txt"]
        );
    }

    #[test]
    fn sort_order_is_pane_state_not_persisted_settings() {
        let mut state = test_state();

        state.dispatch_action(CommanderAction::SelectSortOrder(PaneSortOrder::SizeDesc));

        assert_eq!(state.left_pane.sort_order, PaneSortOrder::SizeDesc);
        assert_eq!(Settings::default(), state.settings);
    }

    #[test]
    fn hidden_entries_are_filtered_unless_setting_is_enabled() {
        let parent = FileEntry {
            display_name: "..".to_owned(),
            virtual_path: fs::ROOTS_LOCATION.to_owned(),
            kind: FileEntryKind::Parent,
            size: None,
            modified: None,
            flags: FileEntryFlags {
                hidden: true,
                read_only: false,
            },
        };
        let entries = vec![
            parent.clone(),
            test_entry("visible"),
            hidden_test_entry("hidden"),
        ];

        let hidden_filtered = filter_hidden_entries(entries.clone(), false);
        assert_eq!(
            hidden_filtered
                .iter()
                .map(|entry| entry.display_name.as_str())
                .collect::<Vec<_>>(),
            ["..", "visible"]
        );

        let hidden_visible = filter_hidden_entries(entries, true);
        assert_eq!(
            hidden_visible
                .iter()
                .map(|entry| entry.display_name.as_str())
                .collect::<Vec<_>>(),
            ["..", "visible", "hidden"]
        );
    }

    #[test]
    fn settings_confirm_toggles_focused_boolean_without_closing_modal() {
        let mut state = test_state();

        state.dispatch_action(CommanderAction::OpenSettings);
        state.dispatch_action(CommanderAction::MoveSelectionDown);

        assert!(state.settings.use_recycle_bin);
        let result = state.dispatch_action(CommanderAction::Confirm);

        assert!(!result.quit_requested);
        assert!(!state.settings.use_recycle_bin);
        assert!(matches!(state.dialog, DialogState::Settings(_)));
    }

    #[test]
    fn settings_confirm_on_quit_row_requests_quit() {
        let mut state = test_state();

        state.dispatch_action(CommanderAction::OpenSettings);
        for _ in 0..5 {
            state.dispatch_action(CommanderAction::MoveSelectionDown);
        }
        assert_eq!(
            state.focused_setting(),
            Some(SettingsModalFocus::CheckForUpdates)
        );
        state.dispatch_action(CommanderAction::MoveSelectionDown);
        let result = state.dispatch_action(CommanderAction::Confirm);

        assert!(result.quit_requested);
        assert_eq!(state.status, "Quit requested.");
    }

    #[test]
    fn selection_movement_wraps_circularly() {
        let mut state = test_state();

        assert_eq!(state.active_pane().selected_index, 1);

        state.dispatch_action(CommanderAction::MoveSelectionUp);
        assert_eq!(state.active_pane().selected_index, 0);

        // wraps from first to last
        state.dispatch_action(CommanderAction::MoveSelectionUp);
        assert_eq!(
            state.active_pane().selected_index,
            state.active_pane().entries.len() - 1
        );

        state.dispatch_action(CommanderAction::MoveSelectionEnd);
        assert_eq!(
            state.active_pane().selected_index,
            state.active_pane().entries.len() - 1
        );

        // wraps from last to first
        state.dispatch_action(CommanderAction::MoveSelectionDown);
        assert_eq!(state.active_pane().selected_index, 0);

        state.dispatch_action(CommanderAction::MoveSelectionHome);
        assert_eq!(state.active_pane().selected_index, 0);
    }

    #[test]
    fn movement_applies_only_to_active_pane() {
        let mut state = test_state();
        let right_initial_selection = state.right_pane.selected_index;

        state.dispatch_action(CommanderAction::MoveSelectionDown);

        assert_eq!(state.left_pane.selected_index, 2);
        assert_eq!(state.right_pane.selected_index, right_initial_selection);
    }

    #[test]
    fn toggle_mark_selected_marks_and_unmarks_focused_item() {
        let mut state = test_state();

        state.dispatch_action(CommanderAction::ToggleMarkSelected);
        assert!(state.left_pane.marked_items.contains("b"));

        state.dispatch_action(CommanderAction::ToggleMarkSelected);
        assert!(state.left_pane.marked_items.is_empty());
    }

    #[test]
    fn operation_sources_use_focused_item_when_nothing_marked() {
        let state = test_state();

        let sources = state.active_operation_sources();

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].display_name, "b");
    }

    #[test]
    fn operation_sources_use_all_marked_items() {
        let mut state = test_state();
        state.dispatch_action(CommanderAction::ToggleMarkSelected);
        state.dispatch_action(CommanderAction::MoveSelectionDown);
        state.dispatch_action(CommanderAction::ToggleMarkSelected);

        let names: Vec<_> = state
            .active_operation_sources()
            .into_iter()
            .map(|entry| entry.display_name)
            .collect();

        assert_eq!(names, ["b", "c"]);
        assert_eq!(state.left_pane.marked_summary().count, 2);
        assert_eq!(state.left_pane.marked_summary().total_size, 2048);
    }

    #[cfg(windows)]
    #[test]
    fn create_shortcut_action_is_available_only_for_exe_files() {
        let mut state = test_state();
        state.left_pane.entries = vec![
            FileEntry {
                display_name: "tool.exe".to_owned(),
                virtual_path: r"C:\Apps\tool.exe".to_owned(),
                kind: FileEntryKind::File,
                size: Some(1024),
                modified: None,
                flags: FileEntryFlags::default(),
            },
            FileEntry {
                display_name: "notes.txt".to_owned(),
                virtual_path: r"C:\Apps\notes.txt".to_owned(),
                kind: FileEntryKind::File,
                size: Some(1024),
                modified: None,
                flags: FileEntryFlags::default(),
            },
        ];

        state.left_pane.selected_index = 0;
        assert!(state.can_create_shortcut_in_other_pane());

        state.left_pane.selected_index = 1;
        assert!(!state.can_create_shortcut_in_other_pane());
    }

    #[cfg(windows)]
    #[test]
    fn create_shortcut_prompts_with_parent_folder_name() {
        let mut state = test_state();
        state.left_pane.entries = vec![FileEntry {
            display_name: "launcher.exe".to_owned(),
            virtual_path: r"C:\Games\Example Game\launcher.exe".to_owned(),
            kind: FileEntryKind::File,
            size: Some(1024),
            modified: None,
            flags: FileEntryFlags::default(),
        }];
        state.left_pane.selected_index = 0;
        state.right_pane.current_location = r"C:\Users\Player\Desktop".to_owned();

        state.dispatch_action(CommanderAction::CreateShortcutInOtherPane);

        let DialogState::TextInput(dialog) = &state.dialog else {
            panic!("shortcut should open text input");
        };
        assert_eq!(dialog.purpose, TextInputPurpose::ShortcutName);
        assert_eq!(dialog.value, "Example Game");
        assert!(state.pending_shortcut_request.is_some());
    }

    #[test]
    fn shortcut_name_adds_lnk_extension_only_when_missing() {
        assert_eq!(
            shortcut_name_with_default_extension("Example Game"),
            "Example Game.lnk"
        );
        assert_eq!(
            shortcut_name_with_default_extension("Example Game.lnk"),
            "Example Game.lnk"
        );
        assert_eq!(
            shortcut_name_with_default_extension("Example Game.LNK"),
            "Example Game.LNK"
        );
        assert_eq!(
            shortcut_name_with_default_extension("Example Game.url"),
            "Example Game.url.lnk"
        );
    }

    #[test]
    fn restore_selection_keeps_matching_path_when_refreshing_entries() {
        let mut pane = PaneState {
            current_location: fs::ROOTS_LOCATION.to_owned(),
            entries: vec![test_entry("a"), test_entry("b"), test_entry("c")],
            selected_index: 1,
            marked_items: BTreeSet::from(["missing".to_owned(), "c".to_owned()]),
            load_state: PaneLoadState::Ready,
            sort_order: PaneSortOrder::default(),
            scroll_offset: 0.0,
            needs_scroll_to_selected: false,
            cached_free_space: None,
            navigation_history: Vec::new(),
        };

        pane.entries = vec![test_entry("c"), test_entry("b")];
        pane.retain_valid_marks();
        pane.restore_selection(Some("b"));

        assert_eq!(pane.selected_index, 1);
        assert!(pane.needs_scroll_to_selected);
        assert!(pane.marked_items.contains("c"));
        assert!(!pane.marked_items.contains("missing"));
    }

    #[test]
    fn restore_selection_requests_scroll_when_selection_falls_back_to_top() {
        let mut pane = PaneState {
            current_location: fs::ROOTS_LOCATION.to_owned(),
            entries: vec![test_entry("missing")],
            selected_index: 0,
            marked_items: BTreeSet::new(),
            load_state: PaneLoadState::Ready,
            sort_order: PaneSortOrder::default(),
            scroll_offset: 420.0,
            needs_scroll_to_selected: false,
            cached_free_space: None,
            navigation_history: Vec::new(),
        };

        pane.entries = vec![test_entry("replacement")];
        pane.restore_selection(Some("missing"));

        assert_eq!(pane.selected_index, 0);
        assert_eq!(pane.scroll_offset, 420.0);
        assert!(pane.needs_scroll_to_selected);
    }

    #[test]
    fn selection_movement_requests_scroll_when_wrapping_at_boundary() {
        let mut state = test_state();
        state.left_pane.selected_index = 0;
        state.left_pane.needs_scroll_to_selected = false;

        state.dispatch_action(CommanderAction::MoveSelectionUp);

        // wraps to last entry and requests scroll
        assert_eq!(
            state.left_pane.selected_index,
            state.left_pane.entries.len() - 1
        );
        assert!(state.left_pane.needs_scroll_to_selected);
    }

    #[test]
    fn set_pane_scroll_offset_updates_offset_and_clears_scroll_to_selected() {
        let mut state = test_state();
        state.left_pane.scroll_offset = 0.0;
        state.left_pane.needs_scroll_to_selected = true;

        state.dispatch_action(CommanderAction::SetPaneScrollOffset {
            pane: PaneSide::Left,
            offset: 76,
        });

        assert_eq!(state.left_pane.scroll_offset, 76.0);
        assert!(!state.left_pane.needs_scroll_to_selected);
    }

    #[test]
    fn copy_to_other_pane_starts_background_transfer() {
        let root = unique_test_dir();
        let source_dir = root.join("source");
        let target_dir = root.join("target");
        std::fs::create_dir_all(&source_dir).expect("create source dir");
        std::fs::create_dir_all(&target_dir).expect("create target dir");
        let source_file = source_dir.join("copy-me.txt");
        std::fs::write(&source_file, b"copy").expect("write source file");

        let mut state = test_state();
        state.left_pane.current_location = source_dir.to_string_lossy().to_string();
        state.left_pane.entries = vec![FileEntry {
            display_name: "copy-me.txt".to_owned(),
            virtual_path: source_file.to_string_lossy().to_string(),
            kind: FileEntryKind::File,
            size: Some(4),
            modified: None,
            flags: FileEntryFlags::default(),
        }];
        state.left_pane.selected_index = 0;
        state.right_pane.current_location = target_dir.to_string_lossy().to_string();
        state.active_pane = PaneSide::Left;

        state.dispatch_action(CommanderAction::CopyToOtherPane);

        assert_eq!(state.transfer_task_panes.len(), 1);
        assert!(matches!(state.dialog, DialogState::OperationProgress(_)));

        wait_for_state_tasks(&mut state);
        assert_eq!(
            std::fs::read(target_dir.join("copy-me.txt")).expect("read copied file"),
            b"copy"
        );

        std::fs::remove_dir_all(&root).expect("remove test dir");
    }

    #[test]
    fn go_back_restores_previous_folder_selection_and_scroll_offset() {
        let root = unique_test_dir();
        let alpha_dir = root.join("alpha");
        let beta_dir = root.join("beta");
        std::fs::create_dir_all(&alpha_dir).expect("create alpha dir");
        std::fs::create_dir_all(&beta_dir).expect("create beta dir");

        let mut state = test_state();
        state.left_pane.current_location = root.to_string_lossy().to_string();
        state.refresh_pane(PaneSide::Left);
        let alpha_index = state
            .left_pane
            .entries
            .iter()
            .position(|entry| entry.virtual_path == alpha_dir.to_string_lossy())
            .expect("alpha should be listed");
        state.left_pane.selected_index = alpha_index;
        state.left_pane.scroll_offset = 96.0;

        state.dispatch_action(CommanderAction::OpenSelected);
        assert_eq!(
            state.left_pane.current_location,
            alpha_dir.to_string_lossy()
        );

        state.left_pane.selected_index = 0;
        state.left_pane.scroll_offset = 0.0;
        state.dispatch_action(CommanderAction::GoBack);

        assert_eq!(state.left_pane.current_location, root.to_string_lossy());
        assert_eq!(state.left_pane.selected_index, alpha_index);
        assert_eq!(state.left_pane.scroll_offset, 96.0);

        std::fs::remove_dir_all(&root).expect("remove test dir");
    }

    #[test]
    fn go_back_without_history_navigates_to_parent_folder() {
        let root = unique_test_dir();
        let child_dir = root.join("child");
        std::fs::create_dir_all(&child_dir).expect("create child dir");

        let mut state = test_state();
        state.left_pane.current_location = child_dir.to_string_lossy().to_string();
        state.refresh_pane(PaneSide::Left);

        state.dispatch_action(CommanderAction::GoBack);

        assert_eq!(state.left_pane.current_location, root.to_string_lossy());
        assert!(state.left_pane.navigation_history.is_empty());

        std::fs::remove_dir_all(&root).expect("remove test dir");
    }

    #[test]
    fn go_back_on_removed_history_location_navigates_to_parent_folder() {
        let root = unique_test_dir();
        let child_dir = root.join("child");
        let grandchild_dir = child_dir.join("grandchild");
        std::fs::create_dir_all(&grandchild_dir).expect("create nested dirs");

        let mut state = test_state();
        state.left_pane.current_location = root.to_string_lossy().to_string();
        state.refresh_pane(PaneSide::Left);

        let child_index = state
            .left_pane
            .entries
            .iter()
            .position(|entry| entry.virtual_path == child_dir.to_string_lossy())
            .expect("child should be listed");
        state.left_pane.selected_index = child_index;
        state.dispatch_action(CommanderAction::OpenSelected);

        let grandchild_index = state
            .left_pane
            .entries
            .iter()
            .position(|entry| entry.virtual_path == grandchild_dir.to_string_lossy())
            .expect("grandchild should be listed");
        state.left_pane.selected_index = grandchild_index;
        state.dispatch_action(CommanderAction::OpenSelected);

        std::fs::remove_dir_all(&child_dir).expect("remove history folder");

        state.dispatch_action(CommanderAction::GoBack);

        assert_eq!(state.left_pane.current_location, root.to_string_lossy());

        std::fs::remove_dir_all(&root).expect("remove test dir");
    }

    fn unique_test_dir() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "pad_commander_state_test_{timestamp}_{}",
            std::process::id()
        ))
    }

    fn wait_for_state_tasks(state: &mut AppState) {
        for _ in 0..100 {
            state.poll_tasks();
            if state.transfer_task_panes.is_empty() && state.delete_tasks.is_empty() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        panic!("state task did not complete");
    }
    fn test_server() -> FtpServer {
        ftp::parse_address("ftp://deck@nas.local:21/").expect("parse server")
    }

    fn with_ftp_session(state: &mut AppState, side: PaneSide, return_location: &str) {
        state.ftp_sessions.insert(
            side,
            FtpSession {
                connection: Arc::new(FtpConnection::new(test_server(), "secret".to_owned())),
                return_location: return_location.to_owned(),
                offer_save_password: false,
            },
        );
    }

    #[test]
    fn command_menu_offers_connect_on_local_pane_and_disconnect_on_server_pane() {
        let mut state = test_state();

        state.dispatch_action(CommanderAction::OpenCommandPalette);
        let DialogState::CommandMenu(menu) = &state.dialog else {
            panic!("command menu should open");
        };
        let actions: Vec<_> = menu
            .entries
            .iter()
            .map(|entry| entry.action.clone())
            .collect();
        assert!(actions.contains(&CommanderAction::ConnectFtp));
        assert!(actions.contains(&CommanderAction::CompressToOtherPane));

        with_ftp_session(&mut state, PaneSide::Left, fs::ROOTS_LOCATION);
        state.dispatch_action(CommanderAction::OpenCommandPalette);
        let DialogState::CommandMenu(menu) = &state.dialog else {
            panic!("command menu should open");
        };
        let actions: Vec<_> = menu
            .entries
            .iter()
            .map(|entry| entry.action.clone())
            .collect();
        assert!(actions.contains(&CommanderAction::DisconnectFtp));
        assert!(!actions.contains(&CommanderAction::CompressToOtherPane));
    }

    #[test]
    fn servers_add_row_opens_address_input() {
        let mut state = test_state();

        state.dispatch_action(CommanderAction::ConnectFtp);
        state.dispatch_action(CommanderAction::Confirm);

        let DialogState::TextInput(dialog) = &state.dialog else {
            panic!("address input should open");
        };
        assert_eq!(dialog.purpose, TextInputPurpose::ConnectFtp);
        assert_eq!(dialog.value, "ftp://");
    }

    #[test]
    fn submitting_an_address_saves_the_server_and_asks_for_password() {
        let mut state = test_state();

        state.apply_text_submission(
            TextInputPurpose::ConnectFtp,
            "ftp://deck@nas.local/".to_owned(),
        );

        assert_eq!(state.settings.ftp_servers, vec![test_server()]);
        assert_eq!(state.pending_ftp_server, Some(test_server()));
        assert!(matches!(
            &state.dialog,
            DialogState::TextInput(dialog) if dialog.purpose == TextInputPurpose::FtpPassword
        ));
    }

    #[test]
    fn removing_a_server_keeps_the_add_row_focusable() {
        let mut state = test_state();
        state.settings.ftp_servers.push(test_server());
        state.dialog = DialogState::FtpServers(ListModalState::default());

        state.dispatch_action(CommanderAction::RemoveFtpServer);

        assert!(state.settings.ftp_servers.is_empty());
        assert_eq!(
            state.dialog,
            DialogState::FtpServers(ListModalState::default())
        );
    }

    #[test]
    fn parent_of_server_root_returns_to_previous_local_location() {
        let mut state = test_state();
        with_ftp_session(&mut state, PaneSide::Left, "C:\\Games");

        assert_eq!(
            state.parent_of(PaneSide::Left, "ftp://nas.local:21/pub"),
            "ftp://nas.local:21/"
        );
        assert_eq!(
            state.parent_of(PaneSide::Left, "ftp://nas.local:21/"),
            "C:\\Games"
        );
        assert!(state.location_exists(PaneSide::Left, "ftp://nas.local:21/pub"));
        assert!(!state.location_exists(PaneSide::Right, "ftp://nas.local:21/pub"));
    }

    #[test]
    fn transfers_between_two_server_panes_are_rejected() {
        let mut state = test_state();
        state.left_pane.current_location = "ftp://nas.local:21/".to_owned();
        state.right_pane.current_location = "ftp://other:21/".to_owned();

        state.dispatch_action(CommanderAction::CopyToOtherPane);

        assert_eq!(state.dialog, DialogState::None);
        assert!(state.status.contains("two servers"));
    }

    #[test]
    fn server_locations_are_not_remembered_for_next_launch() {
        let mut state = test_state();
        state.settings.last_left_pane_location = Some("C:\\Games".to_owned());
        state.left_pane.current_location = "ftp://nas.local:21/".to_owned();

        state.remember_pane_location(PaneSide::Left);

        assert_eq!(
            state.settings.last_left_pane_location.as_deref(),
            Some("C:\\Games")
        );
    }
}
