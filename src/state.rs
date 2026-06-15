use crate::{
    action::{CommanderAction, FocusDirection, TextInputAction, TextInputPurpose},
    fs::{self, LocalFileSystem, VirtualFileSystem},
    platform::windows,
    settings::{self, Settings, UI_SCALE_STEP},
    tasks::{DeleteMode, TaskId, TaskKind, TaskManager, TaskStatus},
};

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
        let mut state = Self {
            settings,
            settings_path,
            left_pane: PaneState::new("Left Pane", left_location),
            right_pane: PaneState::new("Right Pane", right_location),
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
                if !matches!(self.dialog, DialogState::CommandMenu(_)) {
                    self.switch_active_pane();
                    self.status = format!("{} pane active.", self.active_pane_label());
                }
            }
            CommanderAction::MoveSelectionUp => {
                if self.move_command_menu_focus(SelectionMovement::Up) {
                    self.status = "Command menu focus moved.".to_owned();
                } else {
                    self.move_active_selection(SelectionMovement::Up);
                }
            }
            CommanderAction::MoveSelectionDown => {
                if self.move_command_menu_focus(SelectionMovement::Down) {
                    self.status = "Command menu focus moved.".to_owned();
                } else {
                    self.move_active_selection(SelectionMovement::Down)
                }
            }
            CommanderAction::MoveSelectionPageUp => {
                self.move_active_selection(SelectionMovement::PageUp)
            }
            CommanderAction::MoveSelectionPageDown => {
                self.move_active_selection(SelectionMovement::PageDown);
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
                        .map_or(true, |t| t.status.is_finished());
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
                self.begin_text_input(TextInputPurpose::ConnectFtp, String::new())
            }
            CommanderAction::CompressToOtherPane => self.begin_archive_name_input(),
            CommanderAction::DecompressToOtherPane => self.begin_archive_decompression_choices(),
            CommanderAction::OpenFavourites => {
                self.dialog = DialogState::Favourites(FavouritesModalState::default());
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
            CommanderAction::UploadToOtherPane | CommanderAction::DownloadToOtherPane => {
                self.set_operation_todo_status(action)
            }
        }

        DispatchResult::default()
    }

    pub fn vfs_label(&self) -> &'static str {
        fs::scheme_name(&self.local_file_system)
    }

    pub fn task_summary(&self) -> String {
        self.task_manager.summary()
    }

    pub fn poll_tasks(&mut self) {
        for completion in self.task_manager.poll_completed() {
            let modal_is_showing = matches!(self.dialog, DialogState::OperationProgress(id) if id == completion.id);

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
                let is_executable =
                    matches!(ext.as_deref(), Some("exe" | "bat" | "cmd" | "com"));

                if is_executable {
                    let parent = path.parent().unwrap_or(path);
                    match std::process::Command::new(path).current_dir(parent).spawn() {
                        Ok(_) => {
                            self.status =
                                format!("Launched {}.", selected_entry.display_name);
                        }
                        Err(e) => {
                            self.status = format!(
                                "Failed to launch {}: {}",
                                selected_entry.display_name, e
                            );
                        }
                    }
                } else {
                    match windows::open_with_default_app(path) {
                        Ok(()) => {
                            self.status =
                                format!("Opened {}.", selected_entry.display_name);
                        }
                        Err(e) => {
                            self.status = format!(
                                "Failed to open {}: {}",
                                selected_entry.display_name, e
                            );
                        }
                    }
                }
            }
            FileEntryKind::FtpItem | FileEntryKind::Unknown => {
                self.status = format!(
                    "{} cannot be opened by local navigation.",
                    selected_entry.display_name
                );
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

    fn set_operation_todo_status(&mut self, action: CommanderAction) {
        let sources = self.active_operation_sources();
        self.status = if sources.is_empty() {
            format!("{action:?}: no available source items.")
        } else {
            format!(
                "{action:?} would use {} item(s): {}",
                sources.len(),
                sources
                    .iter()
                    .map(|entry| entry.display_name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
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
                    conflict.source_paths,
                    conflict.target_directory,
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
            DialogState::Message { .. } => {
                self.dialog = DialogState::None;
                self.status = "Confirmed placeholder dialog.".to_owned();
            }
            DialogState::OperationProgress(task_id) => {
                let task_id = *task_id;
                let done = self
                    .task_manager
                    .task_by_id(task_id)
                    .map_or(true, |t| t.status.is_finished());
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
            TextInputPurpose::ConnectFtp | TextInputPurpose::FtpProfileField => {
                self.status = "FTP text fields are reserved for the FTP milestone.".to_owned();
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
            self.dialog = DialogState::ShortcutConflict(ShortcutConflictState { request, focused_index: 0 });
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
    }

    fn start_create_shortcut(&mut self, request: ShortcutRequest, overwrite: bool) {
        if request.shortcut_path.exists() && !overwrite {
            self.dialog = DialogState::ShortcutConflict(ShortcutConflictState { request, focused_index: 0 });
            self.status = "Shortcut already exists.".to_owned();
            return;
        }

        let result = windows::create_shortcut(
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

        let source_paths = sources
            .iter()
            .map(|entry| PathBuf::from(&entry.virtual_path))
            .collect::<Vec<_>>();
        let target_directory = PathBuf::from(&target_location);
        let label = format!(
            "{} {} item(s) to {}",
            kind.label(),
            source_paths.len(),
            fs::location_label(&target_location)
        );

        let conflicts: Vec<String> = source_paths
            .iter()
            .filter_map(|src| {
                let name = src.file_name()?;
                let target = target_directory.join(name);
                if target.exists() {
                    Some(name.to_string_lossy().into_owned())
                } else {
                    None
                }
            })
            .collect();

        if !conflicts.is_empty() {
            self.dialog = DialogState::TransferConflict(TransferConflictState {
                kind,
                source_side,
                target_side,
                source_paths,
                target_directory,
                conflicts,
                label,
                focused_index: 0,
            });
            return;
        }

        self.execute_transfer_task(kind, source_side, target_side, source_paths, target_directory, label, false);
    }

    fn execute_transfer_task(
        &mut self,
        kind: TaskKind,
        source_side: PaneSide,
        target_side: PaneSide,
        source_paths: Vec<PathBuf>,
        target_directory: PathBuf,
        label: String,
        overwrite: bool,
    ) {
        let task_id = match kind {
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

    fn start_archive_decompression(&mut self, request: DecompressRequest, password: Option<String>) {
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
            if fs::location_exists(&snapshot.location) {
                self.pane_mut(side).current_location = snapshot.location.clone();
                self.remember_pane_location(side);
                self.refresh_pane(side);
                self.pane_mut(side).restore_snapshot_position(snapshot);
                return;
            }

            let fallback_location = fs::parent_location(&snapshot.location);
            if fs::location_exists(&fallback_location) {
                self.pane_mut(side).current_location = fallback_location;
                self.remember_pane_location(side);
                self.refresh_pane(side);
                return;
            }
        }

        self.go_parent(side);
    }

    fn go_parent(&mut self, side: PaneSide) {
        let parent = fs::parent_location(&self.pane(side).current_location);
        self.pane_mut(side).current_location = parent;
        self.remember_pane_location(side);
        self.refresh_pane(side);
    }

    fn refresh_pane(&mut self, side: PaneSide) {
        let location = self.pane(side).current_location.clone();
        let previous_selection_path = self
            .pane(side)
            .selected_entry()
            .map(|entry| entry.virtual_path.clone());
        let result = self.local_file_system.list_location(&location);
        let show_hidden_files = self.settings.show_hidden_files;
        let pane = self.pane_mut(side);

        pane.cached_free_space = crate::fs::get_free_space(&location);

        match result {
            Ok(entries) => {
                pane.entries = filter_hidden_entries(entries, show_hidden_files);
                pane.sort_entries();
                pane.retain_valid_marks();
                pane.restore_selection(previous_selection_path.as_deref());
                pane.load_state = PaneLoadState::Ready;
                self.status = format!("Loaded {}.", fs::location_label(&location));
            }
            Err(error) => {
                let message = error.user_message();
                pane.entries.clear();
                pane.reset_selection();
                pane.marked_items.clear();
                pane.load_state = PaneLoadState::Error(message.clone());
                self.status = message;
            }
        }
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

    pub fn can_create_shortcut_in_other_pane(&self) -> bool {
        self.active_pane()
            .selected_markable_entry()
            .map(FileEntry::is_exe_file)
            .unwrap_or(false)
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
            | DialogState::Decompress(_)
            | DialogState::SortBy(_)
            | DialogState::OperationProgress(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PaneState {
    pub title: String,
    pub current_location: String,
    pub entries: Vec<FileEntry>,
    pub selected_index: usize,
    pub marked_items: BTreeSet<String>,
    pub load_state: PaneLoadState,
    pub sort_order: PaneSortOrder,
    pub scroll_offset: f32,
    pub needs_scroll_to_selected: bool,
    pub cached_free_space: Option<u64>,
    navigation_history: Vec<NavigationSnapshot>,
}

impl PaneState {
    fn new(title: &str, location: String) -> Self {
        Self {
            title: title.to_owned(),
            current_location: location,
            entries: Vec::new(),
            selected_index: 0,
            marked_items: BTreeSet::new(),
            load_state: PaneLoadState::Loading,
            sort_order: PaneSortOrder::default(),
            scroll_offset: 0.0,
            needs_scroll_to_selected: true,
            cached_free_space: None,
            navigation_history: Vec::new(),
        }
    }

    pub fn selected_entry(&self) -> Option<&FileEntry> {
        self.entries.get(self.selected_index)
    }

    pub fn selected_markable_entry(&self) -> Option<&FileEntry> {
        self.selected_entry()
            .filter(|entry| entry.is_available_source())
    }

    pub fn select_index(&mut self, index: usize) {
        if self.entries.is_empty() {
            self.reset_selection();
        } else {
            self.set_selected_index(index);
        }
    }

    pub fn move_selection(&mut self, movement: SelectionMovement) {
        if self.entries.is_empty() {
            self.reset_selection();
            return;
        }

        let last_index = self.entries.len() - 1;
        let next_index = match movement {
            SelectionMovement::Up => {
                if self.selected_index == 0 { last_index } else { self.selected_index - 1 }
            }
            SelectionMovement::Down => {
                if self.selected_index >= last_index { 0 } else { self.selected_index + 1 }
            }
            SelectionMovement::PageUp => self.selected_index.saturating_sub(PAGE_SIZE),
            SelectionMovement::PageDown => (self.selected_index + PAGE_SIZE).min(last_index),
            SelectionMovement::Home => 0,
            SelectionMovement::End => last_index,
        };
        self.set_selected_index(next_index);
    }

    pub fn marked_summary(&self) -> MarkedSummary {
        if self.marked_items.is_empty() {
            return MarkedSummary::default();
        }

        let mut summary = MarkedSummary::default();

        for entry in self
            .entries
            .iter()
            .filter(|entry| self.marked_items.contains(&entry.virtual_path))
            .filter(|entry| entry.is_available_source())
        {
            summary.count += 1;
            if let Some(size) = entry.size {
                summary.total_size = summary.total_size.saturating_add(size);
            } else {
                summary.has_unknown_size = true;
            }
        }

        summary
    }

    pub fn pane_summary(&self) -> PaneSummary {
        let mut summary = PaneSummary::default();

        for entry in self.entries.iter() {
            if matches!(entry.kind, FileEntryKind::File | FileEntryKind::Archive) {
                summary.file_count += 1;
                if let Some(size) = entry.size {
                    summary.total_size = summary.total_size.saturating_add(size);
                }
            } else if matches!(entry.kind, FileEntryKind::Directory) {
                summary.folder_count += 1;
            }
        }

        summary
    }

    pub fn operation_sources(&self) -> Vec<FileEntry> {
        let marked = self.marked_entries();
        if !marked.is_empty() {
            return marked.into_iter().cloned().collect();
        }

        self.selected_markable_entry()
            .cloned()
            .into_iter()
            .collect()
    }

    fn marked_entries(&self) -> Vec<&FileEntry> {
        self.entries
            .iter()
            .filter(|entry| self.marked_items.contains(&entry.virtual_path))
            .filter(|entry| entry.is_available_source())
            .collect()
    }

    fn retain_valid_marks(&mut self) {
        let valid_paths: BTreeSet<_> = self
            .entries
            .iter()
            .filter(|entry| entry.is_available_source())
            .map(|entry| entry.virtual_path.clone())
            .collect();
        self.marked_items.retain(|path| valid_paths.contains(path));
    }

    fn restore_selection(&mut self, previous_selection_path: Option<&str>) {
        if self.entries.is_empty() {
            self.reset_selection();
            return;
        }

        let selected_index = previous_selection_path
            .and_then(|path| {
                self.entries
                    .iter()
                    .position(|entry| entry.virtual_path == path)
            })
            .unwrap_or(if self.entries.len() > 1 { 1 } else { 0 });
        self.set_selected_index(selected_index);
    }

    fn set_selected_index(&mut self, index: usize) {
        if self.entries.is_empty() {
            self.reset_selection();
            return;
        }

        self.selected_index = index.min(self.entries.len() - 1);
        self.needs_scroll_to_selected = true;
    }

    fn reset_selection(&mut self) {
        self.selected_index = 0;
        self.scroll_offset = 0.0;
        self.needs_scroll_to_selected = false;
    }

    fn sort_entries(&mut self) {
        let sort_order = self.sort_order;
        self.entries.sort_by(|left, right| {
            file_entry_sort_group(left)
                .cmp(&file_entry_sort_group(right))
                .then_with(|| compare_file_entries(left, right, sort_order))
        });
    }

    fn navigation_snapshot(&self) -> NavigationSnapshot {
        NavigationSnapshot {
            location: self.current_location.clone(),
            selected_index: self.selected_index,
            scroll_offset: self.scroll_offset,
        }
    }

    fn restore_snapshot_position(&mut self, snapshot: NavigationSnapshot) {
        if self.entries.is_empty() {
            self.selected_index = 0;
            self.scroll_offset = 0.0;
            self.needs_scroll_to_selected = false;
            return;
        }

        self.selected_index = snapshot.selected_index.min(self.entries.len() - 1);
        self.scroll_offset = snapshot.scroll_offset.max(0.0);
        self.needs_scroll_to_selected = false;
    }
}

#[derive(Clone, Debug, PartialEq)]
struct NavigationSnapshot {
    location: String,
    selected_index: usize,
    scroll_offset: f32,
}

const PAGE_SIZE: usize = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionMovement {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileEntry {
    pub display_name: String,
    pub virtual_path: String,
    pub kind: FileEntryKind,
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub flags: FileEntryFlags,
}

impl FileEntry {
    pub fn type_display_label(&self) -> &'static str {
        match self.kind {
            FileEntryKind::Directory => "Directory",
            FileEntryKind::Drive => "Drive",
            FileEntryKind::Archive => "Archive",
            FileEntryKind::Parent => "Parent",
            FileEntryKind::FtpItem => "FTP",
            FileEntryKind::Unknown => "Unknown",
            FileEntryKind::File => {
                let ext = Path::new(&self.virtual_path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_ascii_lowercase());
                match ext.as_deref() {
                    Some("png") => "PNG Image",
                    Some("jpg") | Some("jpeg") => "JPEG Image",
                    Some("gif") => "GIF Image",
                    Some("bmp") => "Bitmap Image",
                    Some("ico") => "Icon",
                    Some("svg") => "SVG Image",
                    Some("webp") => "WebP Image",
                    Some("tiff") | Some("tif") => "TIFF Image",
                    Some("pdf") => "PDF Document",
                    Some("doc") => "Word Document",
                    Some("docx") => "Word Document",
                    Some("xls") => "Excel Sheet",
                    Some("xlsx") => "Excel Sheet",
                    Some("ppt") | Some("pptx") => "Presentation",
                    Some("odt") => "ODF Document",
                    Some("txt") => "Text File",
                    Some("md") => "Markdown",
                    Some("csv") => "CSV File",
                    Some("json") => "JSON File",
                    Some("xml") => "XML File",
                    Some("yaml") | Some("yml") => "YAML File",
                    Some("toml") => "TOML File",
                    Some("ini") | Some("cfg") | Some("conf") => "Config File",
                    Some("log") => "Log File",
                    Some("rs") => "Rust Source",
                    Some("c") => "C Source",
                    Some("cpp") | Some("cxx") | Some("cc") => "C++ Source",
                    Some("h") | Some("hpp") => "C/C++ Header",
                    Some("py") => "Python Script",
                    Some("js") => "JavaScript",
                    Some("ts") => "TypeScript",
                    Some("html") | Some("htm") => "HTML File",
                    Some("css") => "CSS File",
                    Some("sh") | Some("bash") => "Shell Script",
                    Some("ps1") => "PowerShell Script",
                    Some("bat") | Some("cmd") => "Batch Script",
                    Some("lua") => "Lua Script",
                    Some("exe") => "Executable",
                    Some("dll") => "DLL Library",
                    Some("so") => "Shared Library",
                    Some("sys") => "System File",
                    Some("mp3") | Some("flac") | Some("ogg") | Some("wav") | Some("aac") => {
                        "Audio File"
                    }
                    Some("mp4") | Some("mkv") | Some("avi") | Some("mov") | Some("wmv") => {
                        "Video File"
                    }
                    Some("ttf") | Some("otf") | Some("woff") | Some("woff2") => "Font File",
                    _ => "File",
                }
            }
        }
    }

    pub fn size_label(&self) -> String {
        match self.size {
            Some(size) => Self::size_value_label(size),
            None => String::new(),
        }
    }

    pub fn size_value_label(size: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = 1024 * KB;
        const GB: u64 = 1024 * MB;
        const TB: u64 = 1024 * GB;
        if size >= TB {
            format!("{:.1} TB", size as f64 / TB as f64)
        } else if size >= GB {
            format!("{:.1} GB", size as f64 / GB as f64)
        } else if size >= MB {
            format!("{:.1} MB", size as f64 / MB as f64)
        } else if size >= KB {
            format!("{} KB", size / KB)
        } else {
            format!("{size} B")
        }
    }

    pub fn modified_label(&self) -> String {
        fs::modified_label(self.modified)
    }

    pub fn is_exe_file(&self) -> bool {
        self.kind == FileEntryKind::File
            && Path::new(&self.virtual_path)
                .extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| extension.eq_ignore_ascii_case("exe"))
                .unwrap_or(false)
    }

    pub fn is_available_source(&self) -> bool {
        !matches!(self.kind, FileEntryKind::Parent | FileEntryKind::Drive)
    }
}

fn file_entry_sort_group(entry: &FileEntry) -> u8 {
    match entry.kind {
        FileEntryKind::Parent => 0,
        FileEntryKind::Drive => 1,
        FileEntryKind::Directory => 2,
        FileEntryKind::Archive
        | FileEntryKind::File
        | FileEntryKind::FtpItem
        | FileEntryKind::Unknown => 3,
    }
}

fn compare_file_entries(
    left: &FileEntry,
    right: &FileEntry,
    sort_order: PaneSortOrder,
) -> Ordering {
    match sort_order {
        PaneSortOrder::FilenameAsc => compare_names(left, right),
        PaneSortOrder::FilenameDesc => compare_names(right, left),
        PaneSortOrder::DateAsc => compare_optional_oldest_first(left.modified, right.modified)
            .then_with(|| compare_names(left, right)),
        PaneSortOrder::DateDesc => compare_optional_newest_first(left.modified, right.modified)
            .then_with(|| compare_names(left, right)),
        PaneSortOrder::SizeAsc => compare_optional_smallest_first(left.size, right.size)
            .then_with(|| compare_names(left, right)),
        PaneSortOrder::SizeDesc => compare_optional_largest_first(left.size, right.size)
            .then_with(|| compare_names(left, right)),
    }
}

fn compare_names(left: &FileEntry, right: &FileEntry) -> Ordering {
    left.display_name
        .to_lowercase()
        .cmp(&right.display_name.to_lowercase())
}

fn compare_optional_oldest_first<T: Ord>(left: Option<T>, right: Option<T>) -> Ordering {
    compare_optional_known_first(left, right, |left, right| left.cmp(&right))
}

fn compare_optional_newest_first<T: Ord>(left: Option<T>, right: Option<T>) -> Ordering {
    compare_optional_known_first(left, right, |left, right| right.cmp(&left))
}

fn compare_optional_smallest_first<T: Ord>(left: Option<T>, right: Option<T>) -> Ordering {
    compare_optional_known_first(left, right, |left, right| left.cmp(&right))
}

fn compare_optional_largest_first<T: Ord>(left: Option<T>, right: Option<T>) -> Ordering {
    compare_optional_known_first(left, right, |left, right| right.cmp(&left))
}

fn compare_optional_known_first<T: Ord>(
    left: Option<T>,
    right: Option<T>,
    compare_known: impl FnOnce(T, T) -> Ordering,
) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => compare_known(left, right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MarkedSummary {
    pub count: usize,
    pub total_size: u64,
    pub has_unknown_size: bool,
}

impl MarkedSummary {
    pub fn size_label(self) -> String {
        if self.count == 0 {
            return "0 items".to_owned();
        }

        let size_label = FileEntry::size_value_label(self.total_size);
        if self.has_unknown_size {
            format!("{} item(s), {size_label} + unknown", self.count)
        } else {
            format!("{} item(s), {size_label}", self.count)
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PaneSummary {
    pub file_count: usize,
    pub folder_count: usize,
    pub total_size: u64,
}

impl PaneSummary {
    pub fn format(&self) -> String {
        let size_label = FileEntry::size_value_label(self.total_size);
        let file_label = if self.file_count == 1 { "file" } else { "files" };
        let folder_label = if self.folder_count == 1 { "folder" } else { "folders" };
        format!(
            "{} {}, {} {} | {size_label}",
            self.file_count, file_label, self.folder_count, folder_label
        )
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileEntryKind {
    File,
    Directory,
    Drive,
    Archive,
    Parent,
    FtpItem,
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FileEntryFlags {
    pub hidden: bool,
    pub read_only: bool,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaneLoadState {
    Ready,
    Loading,
    Error(String),
}

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DialogState {
    None,
    Message { title: String, message: String },
    TextInput(TextInputDialogState),
    DeleteConfirm(DeleteConfirmationState),
    ShortcutConflict(ShortcutConflictState),
    TransferConflict(TransferConflictState),
    CommandMenu(CommandMenuState),
    Settings(SettingsModalState),
    Favourites(FavouritesModalState),
    Decompress(DecompressModalState),
    SortBy(SortByModalState),
    OperationProgress(TaskId),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SettingsModalState {
    pub focused_index: usize,
}

impl SettingsModalState {
    pub fn move_focus(&mut self, movement: SelectionMovement) {
        const LAST_INDEX: usize = 3;

        self.focused_index = match movement {
            SelectionMovement::Up | SelectionMovement::PageUp => {
                self.focused_index.saturating_sub(1)
            }
            SelectionMovement::Down | SelectionMovement::PageDown => {
                (self.focused_index + 1).min(LAST_INDEX)
            }
            SelectionMovement::Home => 0,
            SelectionMovement::End => LAST_INDEX,
        };
    }

    pub fn focused_setting(&self) -> SettingsModalFocus {
        match self.focused_index {
            1 => SettingsModalFocus::UseRecycleBin,
            2 => SettingsModalFocus::ShowHiddenFiles,
            3 => SettingsModalFocus::Quit,
            _ => SettingsModalFocus::UiScale,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsModalFocus {
    UiScale,
    UseRecycleBin,
    ShowHiddenFiles,
    Quit,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FavouritesModalState {
    pub focused_index: usize,
}

impl FavouritesModalState {
    pub fn move_focus(&mut self, movement: SelectionMovement, favourite_count: usize) {
        if favourite_count == 0 {
            self.focused_index = 0;
            return;
        }

        let last_index = favourite_count - 1;
        self.focused_index = match movement {
            SelectionMovement::Up | SelectionMovement::PageUp => {
                self.focused_index.saturating_sub(1)
            }
            SelectionMovement::Down | SelectionMovement::PageDown => {
                (self.focused_index + 1).min(last_index)
            }
            SelectionMovement::Home => 0,
            SelectionMovement::End => last_index,
        };
    }

    pub fn clamp_focus(&mut self, favourite_count: usize) {
        if favourite_count == 0 {
            self.focused_index = 0;
        } else {
            self.focused_index = self.focused_index.min(favourite_count - 1);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecompressModalState {
    pub focused_index: usize,
    pub archive: FileEntry,
    pub archive_stem: String,
    pub source_side: PaneSide,
}

impl DecompressModalState {
    pub fn choices(&self) -> [DecompressChoice; DecompressChoice::COUNT] {
        [
            DecompressChoice::Here,
            DecompressChoice::HereToArchiveFolder,
            DecompressChoice::OtherPane,
            DecompressChoice::OtherPaneToArchiveFolder,
        ]
    }

    pub fn move_focus(&mut self, movement: SelectionMovement) {
        let last_index = DecompressChoice::COUNT - 1;
        self.focused_index = match movement {
            SelectionMovement::Up | SelectionMovement::PageUp => {
                self.focused_index.saturating_sub(1)
            }
            SelectionMovement::Down | SelectionMovement::PageDown => {
                (self.focused_index + 1).min(last_index)
            }
            SelectionMovement::Home => 0,
            SelectionMovement::End => last_index,
        };
    }

    fn selected_request(&self) -> DecompressRequest {
        let choice = self.choices()[self.focused_index.min(DecompressChoice::COUNT - 1)];
        DecompressRequest {
            archive: self.archive.clone(),
            archive_stem: self.archive_stem.clone(),
            target_side: choice.target_side(self.source_side),
            create_archive_folder: choice.create_archive_folder(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecompressChoice {
    Here,
    HereToArchiveFolder,
    OtherPane,
    OtherPaneToArchiveFolder,
}

impl DecompressChoice {
    pub const COUNT: usize = 4;

    pub fn label(self, archive_stem: &str) -> String {
        match self {
            Self::Here => "Decompress here".to_owned(),
            Self::HereToArchiveFolder => format!("Decompress here to {archive_stem}"),
            Self::OtherPane => "Decompress on other pane".to_owned(),
            Self::OtherPaneToArchiveFolder => {
                format!("Decompress on other pane to {archive_stem}")
            }
        }
    }

    fn target_side(self, source_side: PaneSide) -> PaneSide {
        match self {
            Self::Here | Self::HereToArchiveFolder => source_side,
            Self::OtherPane | Self::OtherPaneToArchiveFolder => source_side.other(),
        }
    }

    fn create_archive_folder(self) -> bool {
        matches!(
            self,
            Self::HereToArchiveFolder | Self::OtherPaneToArchiveFolder
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DecompressRequest {
    archive: FileEntry,
    archive_stem: String,
    target_side: PaneSide,
    create_archive_folder: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SortByModalState {
    pub focused_index: usize,
}

impl SortByModalState {
    pub fn new(current_sort_order: PaneSortOrder) -> Self {
        Self {
            focused_index: Self::index_for_sort_order(current_sort_order),
        }
    }

    pub fn choices(&self) -> [PaneSortOrder; PaneSortOrder::COUNT] {
        [
            PaneSortOrder::FilenameAsc,
            PaneSortOrder::FilenameDesc,
            PaneSortOrder::DateAsc,
            PaneSortOrder::DateDesc,
            PaneSortOrder::SizeAsc,
            PaneSortOrder::SizeDesc,
        ]
    }

    pub fn move_focus(&mut self, movement: SelectionMovement) {
        let last_index = PaneSortOrder::COUNT - 1;
        self.focused_index = match movement {
            SelectionMovement::Up | SelectionMovement::PageUp => {
                self.focused_index.saturating_sub(1)
            }
            SelectionMovement::Down | SelectionMovement::PageDown => {
                (self.focused_index + 1).min(last_index)
            }
            SelectionMovement::Home => 0,
            SelectionMovement::End => last_index,
        };
    }

    pub fn selected_sort_order(&self) -> PaneSortOrder {
        self.choices()[self.focused_index.min(PaneSortOrder::COUNT - 1)]
    }

    fn index_for_sort_order(sort_order: PaneSortOrder) -> usize {
        match sort_order {
            PaneSortOrder::FilenameAsc => 0,
            PaneSortOrder::FilenameDesc => 1,
            PaneSortOrder::DateAsc => 2,
            PaneSortOrder::DateDesc => 3,
            PaneSortOrder::SizeAsc => 4,
            PaneSortOrder::SizeDesc => 5,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandMenuState {
    pub focused_index: usize,
    pub entries: Vec<CommandMenuEntry>,
}

impl Default for CommandMenuState {
    fn default() -> Self {
        Self::new(false)
    }
}

impl CommandMenuState {
    pub fn new(include_create_shortcut: bool) -> Self {
        let mut entries = vec![
            CommandMenuEntry::new("Create Folder", CommanderAction::CreateFolder),
            CommandMenuEntry::new("Create Text File", CommanderAction::CreateTextFile),
        ];

        if include_create_shortcut {
            entries.push(CommandMenuEntry::new(
                "Create Shortcut",
                CommanderAction::CreateShortcutInOtherPane,
            ));
        }

        entries.extend([
            CommandMenuEntry::new("Copy", CommanderAction::CopyToOtherPane),
            CommandMenuEntry::new("Move", CommanderAction::MoveToOtherPane),
            CommandMenuEntry::new("Compress", CommanderAction::CompressToOtherPane),
            CommandMenuEntry::new("Decompress", CommanderAction::DecompressToOtherPane),
            CommandMenuEntry::new("Sort By", CommanderAction::OpenSortBy),
            CommandMenuEntry::new("Rename", CommanderAction::Rename),
            CommandMenuEntry::new("Delete", CommanderAction::Delete),
            CommandMenuEntry::new("Cancel Task", CommanderAction::CancelActiveTask),
        ]);

        Self {
            focused_index: 0,
            entries,
        }
    }

    pub fn move_focus(&mut self, movement: SelectionMovement) {
        if self.entries.is_empty() {
            self.focused_index = 0;
            return;
        }

        let last_index = self.entries.len() - 1;
        self.focused_index = match movement {
            SelectionMovement::Up | SelectionMovement::PageUp => {
                self.focused_index.saturating_sub(1)
            }
            SelectionMovement::Down | SelectionMovement::PageDown => {
                (self.focused_index + 1).min(last_index)
            }
            SelectionMovement::Home => 0,
            SelectionMovement::End => last_index,
        };
    }

    pub fn selected_action(&self) -> CommanderAction {
        self.entries
            .get(self.focused_index)
            .map(|entry| entry.action.clone())
            .unwrap_or(CommanderAction::Cancel)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandMenuEntry {
    pub label: &'static str,
    pub action: CommanderAction,
}

impl CommandMenuEntry {
    fn new(label: &'static str, action: CommanderAction) -> Self {
        Self { label, action }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShortcutConflictState {
    request: ShortcutRequest,
    pub focused_index: usize,
}

impl ShortcutConflictState {
    pub fn shortcut_path(&self) -> &Path {
        &self.request.shortcut_path
    }

    pub fn target_path(&self) -> &Path {
        &self.request.target_path
    }

    pub fn move_focus(&mut self, movement: SelectionMovement) {
        self.focused_index = match movement {
            SelectionMovement::Up | SelectionMovement::PageUp | SelectionMovement::Home => 0,
            SelectionMovement::Down | SelectionMovement::PageDown | SelectionMovement::End => 1,
        };
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingShortcutRequest {
    target_path: PathBuf,
    target_directory: PathBuf,
    working_directory: Option<PathBuf>,
    icon_path: Option<PathBuf>,
    target_side: PaneSide,
}

impl PendingShortcutRequest {
    fn default_shortcut_name(&self) -> String {
        self.working_directory
            .as_deref()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().trim().to_owned())
            .filter(|name| !name.is_empty())
            .or_else(|| {
                self.target_path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().trim().to_owned())
                    .filter(|stem| !stem.is_empty())
            })
            .unwrap_or_else(|| "Shortcut".to_owned())
    }

    fn into_shortcut_request(self, shortcut_name: String) -> ShortcutRequest {
        ShortcutRequest {
            target_path: self.target_path,
            shortcut_path: self.target_directory.join(shortcut_name),
            working_directory: self.working_directory,
            icon_path: self.icon_path,
            target_side: self.target_side,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShortcutRequest {
    target_path: PathBuf,
    shortcut_path: PathBuf,
    working_directory: Option<PathBuf>,
    icon_path: Option<PathBuf>,
    target_side: PaneSide,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteConfirmationState {
    pub pane: PaneSide,
    pub items: Vec<FileEntry>,
    pub focused_index: usize,
}

impl DeleteConfirmationState {
    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    pub fn examples(&self) -> String {
        self.items
            .iter()
            .take(5)
            .map(|entry| entry.display_name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn move_focus(&mut self, movement: SelectionMovement) {
        self.focused_index = match movement {
            SelectionMovement::Up | SelectionMovement::PageUp | SelectionMovement::Home => 0,
            SelectionMovement::Down | SelectionMovement::PageDown | SelectionMovement::End => 1,
        };
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferConflictState {
    pub kind: TaskKind,
    pub source_side: PaneSide,
    pub target_side: PaneSide,
    pub source_paths: Vec<PathBuf>,
    pub target_directory: PathBuf,
    pub conflicts: Vec<String>,
    pub label: String,
    pub focused_index: usize,
}

impl TransferConflictState {
    pub fn conflict_examples(&self) -> String {
        self.conflicts
            .iter()
            .take(3)
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn move_focus(&mut self, movement: SelectionMovement) {
        self.focused_index = match movement {
            SelectionMovement::Up | SelectionMovement::PageUp | SelectionMovement::Home => 0,
            SelectionMovement::Down | SelectionMovement::PageDown | SelectionMovement::End => 1,
        };
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextInputDialogState {
    pub purpose: TextInputPurpose,
    pub title: String,
    pub value: String,
    pub cursor: usize,
    pub layer: KeyboardLayer,
    pub focused_row: usize,
    pub focused_col: usize,
    pub validation_error: Option<String>,
}

impl TextInputDialogState {
    pub fn new(purpose: TextInputPurpose, initial_value: String) -> Self {
        let mut dialog = Self {
            purpose,
            title: purpose.title().to_owned(),
            cursor: initial_value.chars().count(),
            value: initial_value,
            layer: KeyboardLayer::Lowercase,
            focused_row: 0,
            focused_col: 0,
            validation_error: None,
        };
        dialog.validate();
        dialog
    }

    pub fn apply_action(&mut self, action: TextInputAction) -> TextInputOutcome {
        self.validation_error = None;

        match action {
            TextInputAction::Begin { .. } | TextInputAction::Cancel => TextInputOutcome::Editing,
            TextInputAction::Insert(text) => {
                self.insert_text(&text);
                self.validate();
                TextInputOutcome::Editing
            }
            TextInputAction::Backspace => {
                self.backspace();
                self.validate();
                TextInputOutcome::Editing
            }
            TextInputAction::Clear => {
                self.value.clear();
                self.cursor = 0;
                self.validate();
                TextInputOutcome::Editing
            }
            TextInputAction::CursorLeft => {
                self.cursor = self.cursor.saturating_sub(1);
                TextInputOutcome::Editing
            }
            TextInputAction::CursorRight => {
                self.cursor = (self.cursor + 1).min(self.value.chars().count());
                TextInputOutcome::Editing
            }
            TextInputAction::MoveKeyFocus(direction) => {
                self.move_key_focus(direction);
                TextInputOutcome::Editing
            }
            TextInputAction::PressFocusedKey => self.press_focused_key(),
            TextInputAction::ToggleShift => {
                self.layer = match self.layer {
                    KeyboardLayer::Lowercase => KeyboardLayer::Uppercase,
                    KeyboardLayer::Uppercase => KeyboardLayer::Lowercase,
                    KeyboardLayer::Symbols => KeyboardLayer::Uppercase,
                };
                self.clamp_focus();
                TextInputOutcome::Editing
            }
            TextInputAction::ToggleSymbols => {
                self.layer = match self.layer {
                    KeyboardLayer::Symbols => KeyboardLayer::Lowercase,
                    _ => KeyboardLayer::Symbols,
                };
                self.clamp_focus();
                TextInputOutcome::Editing
            }
            TextInputAction::Submit => self.submit(),
        }
    }

    pub fn keys(&self) -> &'static [&'static [OnScreenKey]] {
        self.layer.keys()
    }

    pub fn focused_key(&self) -> OnScreenKey {
        let row = self.keys()[self.focused_row];
        row[self.focused_col.min(row.len() - 1)]
    }

    pub fn display_value_with_cursor(&self) -> String {
        let mut rendered = String::new();
        for (index, ch) in self.value.chars().enumerate() {
            if index == self.cursor {
                rendered.push('|');
            }
            rendered.push(ch);
        }
        if self.cursor == self.value.chars().count() {
            rendered.push('|');
        }
        rendered
    }

    fn press_focused_key(&mut self) -> TextInputOutcome {
        match self.focused_key() {
            OnScreenKey::Text(text) => self.apply_action(TextInputAction::Insert(text.to_owned())),
            OnScreenKey::Space => self.apply_action(TextInputAction::Insert(" ".to_owned())),
            OnScreenKey::Backspace => self.apply_action(TextInputAction::Backspace),
            OnScreenKey::Clear => self.apply_action(TextInputAction::Clear),
            OnScreenKey::CursorLeft => self.apply_action(TextInputAction::CursorLeft),
            OnScreenKey::CursorRight => self.apply_action(TextInputAction::CursorRight),
            OnScreenKey::Shift => self.apply_action(TextInputAction::ToggleShift),
            OnScreenKey::Symbols => self.apply_action(TextInputAction::ToggleSymbols),
            OnScreenKey::Confirm => self.apply_action(TextInputAction::Submit),
            OnScreenKey::Cancel => TextInputOutcome::Cancelled,
        }
    }

    fn submit(&mut self) -> TextInputOutcome {
        self.validate();
        if let Some(error) = &self.validation_error {
            return TextInputOutcome::Blocked(error.clone());
        }

        TextInputOutcome::Submitted {
            purpose: self.purpose,
            value: self.value.trim().to_owned(),
        }
    }

    fn insert_text(&mut self, text: &str) {
        let byte_index = char_to_byte_index(&self.value, self.cursor);
        self.value.insert_str(byte_index, text);
        self.cursor += text.chars().count();
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }

        let start = char_to_byte_index(&self.value, self.cursor - 1);
        let end = char_to_byte_index(&self.value, self.cursor);
        self.value.replace_range(start..end, "");
        self.cursor -= 1;
    }

    fn move_key_focus(&mut self, direction: FocusDirection) {
        match direction {
            FocusDirection::Up => {
                self.focused_row = self.focused_row.saturating_sub(1);
            }
            FocusDirection::Down => {
                self.focused_row = (self.focused_row + 1).min(self.keys().len() - 1);
            }
            FocusDirection::Left => {
                self.focused_col = self.focused_col.saturating_sub(1);
            }
            FocusDirection::Right => {
                self.focused_col =
                    (self.focused_col + 1).min(self.keys()[self.focused_row].len() - 1);
            }
        }
        self.clamp_focus();
    }

    fn clamp_focus(&mut self) {
        self.focused_row = self.focused_row.min(self.keys().len() - 1);
        self.focused_col = self
            .focused_col
            .min(self.keys()[self.focused_row].len() - 1);
    }

    fn validate(&mut self) {
        self.validation_error = validate_text_input(self.purpose, &self.value);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyboardLayer {
    Lowercase,
    Uppercase,
    Symbols,
}

impl KeyboardLayer {
    fn keys(self) -> &'static [&'static [OnScreenKey]] {
        match self {
            KeyboardLayer::Lowercase => LOWERCASE_KEYS,
            KeyboardLayer::Uppercase => UPPERCASE_KEYS,
            KeyboardLayer::Symbols => SYMBOL_KEYS,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnScreenKey {
    Text(&'static str),
    Space,
    Backspace,
    Clear,
    CursorLeft,
    CursorRight,
    Shift,
    Symbols,
    Confirm,
    Cancel,
}

impl OnScreenKey {
    pub fn label(self) -> &'static str {
        match self {
            OnScreenKey::Text(text) => text,
            OnScreenKey::Space => "Space",
            OnScreenKey::Backspace => "Back",
            OnScreenKey::Clear => "Clear",
            OnScreenKey::CursorLeft => "<",
            OnScreenKey::CursorRight => ">",
            OnScreenKey::Shift => "Shift",
            OnScreenKey::Symbols => "Sym",
            OnScreenKey::Confirm => "OK",
            OnScreenKey::Cancel => "Cancel",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TextInputOutcome {
    Editing,
    Cancelled,
    Blocked(String),
    Submitted {
        purpose: TextInputPurpose,
        value: String,
    },
}

impl TextInputPurpose {
    pub fn title(self) -> &'static str {
        match self {
            TextInputPurpose::CreateFolder => "Create Folder",
            TextInputPurpose::CreateFile => "Create File",
            TextInputPurpose::CreateTextFile => "Create Text File",
            TextInputPurpose::Rename => "Rename",
            TextInputPurpose::ConnectFtp => "Connect FTP",
            TextInputPurpose::ArchiveName => "Archive Name",
            TextInputPurpose::ArchiveDestination => "Extract To Folder",
            TextInputPurpose::ArchivePassword => "Archive Password",
            TextInputPurpose::ShortcutName => "Shortcut Name",
            TextInputPurpose::FtpProfileField => "FTP Profile Field",
        }
    }

    fn uses_windows_filename_validation(self) -> bool {
        matches!(
            self,
            TextInputPurpose::CreateFolder
                | TextInputPurpose::CreateFile
                | TextInputPurpose::CreateTextFile
                | TextInputPurpose::Rename
                | TextInputPurpose::ArchiveName
                | TextInputPurpose::ArchiveDestination
                | TextInputPurpose::ShortcutName
        )
    }
}

const LOWERCASE_KEYS: &[&[OnScreenKey]] = &[
    &[
        OnScreenKey::Text("q"),
        OnScreenKey::Text("w"),
        OnScreenKey::Text("e"),
        OnScreenKey::Text("r"),
        OnScreenKey::Text("t"),
        OnScreenKey::Text("y"),
        OnScreenKey::Text("u"),
        OnScreenKey::Text("i"),
        OnScreenKey::Text("o"),
        OnScreenKey::Text("p"),
        OnScreenKey::Backspace,
    ],
    &[
        OnScreenKey::Text("a"),
        OnScreenKey::Text("s"),
        OnScreenKey::Text("d"),
        OnScreenKey::Text("f"),
        OnScreenKey::Text("g"),
        OnScreenKey::Text("h"),
        OnScreenKey::Text("j"),
        OnScreenKey::Text("k"),
        OnScreenKey::Text("l"),
    ],
    &[
        OnScreenKey::Shift,
        OnScreenKey::Text("z"),
        OnScreenKey::Text("x"),
        OnScreenKey::Text("c"),
        OnScreenKey::Text("v"),
        OnScreenKey::Text("b"),
        OnScreenKey::Text("n"),
        OnScreenKey::Text("m"),
        OnScreenKey::Clear,
    ],
    &[
        OnScreenKey::Symbols,
        OnScreenKey::CursorLeft,
        OnScreenKey::Space,
        OnScreenKey::CursorRight,
        OnScreenKey::Cancel,
        OnScreenKey::Confirm,
    ],
];

const UPPERCASE_KEYS: &[&[OnScreenKey]] = &[
    &[
        OnScreenKey::Text("Q"),
        OnScreenKey::Text("W"),
        OnScreenKey::Text("E"),
        OnScreenKey::Text("R"),
        OnScreenKey::Text("T"),
        OnScreenKey::Text("Y"),
        OnScreenKey::Text("U"),
        OnScreenKey::Text("I"),
        OnScreenKey::Text("O"),
        OnScreenKey::Text("P"),
        OnScreenKey::Backspace,
    ],
    &[
        OnScreenKey::Text("A"),
        OnScreenKey::Text("S"),
        OnScreenKey::Text("D"),
        OnScreenKey::Text("F"),
        OnScreenKey::Text("G"),
        OnScreenKey::Text("H"),
        OnScreenKey::Text("J"),
        OnScreenKey::Text("K"),
        OnScreenKey::Text("L"),
    ],
    &[
        OnScreenKey::Shift,
        OnScreenKey::Text("Z"),
        OnScreenKey::Text("X"),
        OnScreenKey::Text("C"),
        OnScreenKey::Text("V"),
        OnScreenKey::Text("B"),
        OnScreenKey::Text("N"),
        OnScreenKey::Text("M"),
        OnScreenKey::Clear,
    ],
    LOWERCASE_KEYS[3],
];

const SYMBOL_KEYS: &[&[OnScreenKey]] = &[
    &[
        OnScreenKey::Text("1"),
        OnScreenKey::Text("2"),
        OnScreenKey::Text("3"),
        OnScreenKey::Text("4"),
        OnScreenKey::Text("5"),
        OnScreenKey::Text("6"),
        OnScreenKey::Text("7"),
        OnScreenKey::Text("8"),
        OnScreenKey::Text("9"),
        OnScreenKey::Text("0"),
        OnScreenKey::Backspace,
    ],
    &[
        OnScreenKey::Text("-"),
        OnScreenKey::Text("_"),
        OnScreenKey::Text("."),
        OnScreenKey::Text(","),
        OnScreenKey::Text(":"),
        OnScreenKey::Text("/"),
        OnScreenKey::Text("\\"),
        OnScreenKey::Text("("),
        OnScreenKey::Text(")"),
    ],
    &[
        OnScreenKey::Shift,
        OnScreenKey::Text("#"),
        OnScreenKey::Text("@"),
        OnScreenKey::Text("+"),
        OnScreenKey::Text("="),
        OnScreenKey::Text("!"),
        OnScreenKey::Text("&"),
        OnScreenKey::Text(";"),
        OnScreenKey::Clear,
    ],
    LOWERCASE_KEYS[3],
];

fn validate_text_input(purpose: TextInputPurpose, value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Some("Name cannot be empty.".to_owned());
    }

    if purpose.uses_windows_filename_validation() {
        validate_windows_filename(trimmed)
    } else {
        None
    }
}

fn validate_windows_filename(value: &str) -> Option<String> {
    const INVALID_CHARS: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    if let Some(ch) = value.chars().find(|ch| INVALID_CHARS.contains(ch)) {
        return Some(format!("Windows filenames cannot contain '{ch}'."));
    }

    if value.ends_with('.') || value.ends_with(' ') {
        return Some("Windows filenames cannot end with a space or period.".to_owned());
    }

    let stem = value
        .split('.')
        .next()
        .unwrap_or(value)
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        return Some(format!("'{value}' is a reserved Windows filename."));
    }

    None
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

fn char_to_byte_index(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(value.len())
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
    use std::time::Duration;

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
        AppState {
            settings: Settings::default(),
            settings_path: std::env::temp_dir().join("pad_commander_test_settings.json"),
            left_pane: PaneState {
                title: "Left".to_owned(),
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
                title: "Right".to_owned(),
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
        }
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
        state.dispatch_action(CommanderAction::MoveSelectionDown);
        state.dispatch_action(CommanderAction::MoveSelectionDown);
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
            title: "Test".to_owned(),
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
            title: "Test".to_owned(),
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
    fn text_dialog_inserts_at_cursor_and_backspaces() {
        let mut dialog = TextInputDialogState::new(TextInputPurpose::CreateFile, "ab".to_owned());

        dialog.apply_action(TextInputAction::CursorLeft);
        dialog.apply_action(TextInputAction::Insert("X".to_owned()));
        assert_eq!(dialog.value, "aXb");

        dialog.apply_action(TextInputAction::Backspace);
        assert_eq!(dialog.value, "ab");
    }

    #[test]
    fn text_dialog_blocks_windows_invalid_filename_characters() {
        let mut dialog = TextInputDialogState::new(TextInputPurpose::CreateFolder, String::new());
        dialog.apply_action(TextInputAction::Insert("bad:name".to_owned()));

        let outcome = dialog.apply_action(TextInputAction::Submit);

        assert!(matches!(outcome, TextInputOutcome::Blocked(_)));
        assert!(dialog.validation_error.is_some());
    }

    #[test]
    fn text_dialog_submits_valid_filename() {
        let mut dialog = TextInputDialogState::new(TextInputPurpose::Rename, "notes".to_owned());

        let outcome = dialog.apply_action(TextInputAction::Submit);

        assert_eq!(
            outcome,
            TextInputOutcome::Submitted {
                purpose: TextInputPurpose::Rename,
                value: "notes".to_owned()
            }
        );
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
}
