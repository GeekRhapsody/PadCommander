use crate::{
    action::{CommanderAction, FocusDirection, TextInputAction, TextInputPurpose},
    fs::{self, LocalFileSystem, VirtualFileSystem},
    settings::Settings,
    tasks::{TaskKind, TaskManager, TaskStatus},
};

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    time::SystemTime,
};

const SCROLL_ROW_HEIGHT: f32 = 29.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaneSide {
    Left,
    Right,
}

pub struct AppState {
    pub settings: Settings,
    pub left_pane: PaneState,
    pub right_pane: PaneState,
    pub active_pane: PaneSide,
    pub status: String,
    pub dialog: DialogState,
    pub show_input_help: bool,
    pub local_file_system: LocalFileSystem,
    pub task_manager: TaskManager,
    pub delete_task_panes: BTreeMap<u64, PaneSide>,
}

impl AppState {
    pub fn new() -> Self {
        let local_file_system = LocalFileSystem;
        let initial_location = local_file_system.initial_location();
        let mut state = Self {
            settings: Settings::default(),
            left_pane: PaneState::new("Left Pane", initial_location.clone()),
            right_pane: PaneState::new("Right Pane", initial_location),
            active_pane: PaneSide::Left,
            status: "Ready. F1 or View opens input help. Ctrl+Q quits during development."
                .to_owned(),
            dialog: DialogState::None,
            show_input_help: false,
            local_file_system,
            task_manager: TaskManager::default(),
            delete_task_panes: BTreeMap::new(),
        };

        state.refresh_pane(PaneSide::Left);
        state.refresh_pane(PaneSide::Right);
        state
    }

    pub fn active_pane_label(&self) -> &'static str {
        match self.active_pane {
            PaneSide::Left => "Left",
            PaneSide::Right => "Right",
        }
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
                self.switch_active_pane();
                self.status = format!("{} pane active.", self.active_pane_label());
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
                    self.confirm_dialog();
                } else {
                    self.open_or_confirm_selected();
                }
            }
            CommanderAction::GoParent => {
                self.go_parent(self.active_pane);
            }
            CommanderAction::Refresh => {
                self.refresh_pane(self.active_pane);
            }
            CommanderAction::ToggleMarkSelected => self.toggle_mark_selected(),
            CommanderAction::Cancel => {
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
                } else if !self.cancel_active_task() {
                    self.status = "Cancel/back is reserved for navigation history.".to_owned();
                }
            }
            CommanderAction::Confirm => self.confirm_dialog(),
            CommanderAction::OpenSettings => {
                self.dialog = DialogState::Message {
                    title: "Settings".to_owned(),
                    message: "Settings are reserved for a future milestone.".to_owned(),
                };
                self.status = "Settings are reserved for a future milestone.".to_owned();
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
            CommanderAction::CompressToOtherPane => {
                self.begin_text_input(TextInputPurpose::ArchiveName, String::new())
            }
            CommanderAction::OpenFavourites => {
                self.status = "Favourites are reserved for a future milestone.".to_owned();
            }
            CommanderAction::OpenCommandPalette => {
                self.dialog = DialogState::CommandMenu(CommandMenuState::default());
                self.status = "Command menu opened.".to_owned();
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
            CommanderAction::StartDemoTask => self.start_demo_task(false),
            CommanderAction::StartFailingDemoTask => self.start_demo_task(true),
            CommanderAction::CancelActiveTask => {
                if !self.cancel_active_task() {
                    self.status = "No cancellable task is active.".to_owned();
                }
            }
            CommanderAction::CopyToOtherPane
            | CommanderAction::MoveToOtherPane
            | CommanderAction::DecompressToOtherPane
            | CommanderAction::UploadToOtherPane
            | CommanderAction::DownloadToOtherPane
            | CommanderAction::CreateShortcutInOtherPane => self.set_operation_todo_status(action),
            action => self.set_todo_status(action),
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
            if let Some(side) = self.delete_task_panes.remove(&completion.id) {
                self.refresh_pane(side);
                match completion.result {
                    Ok(data) if completion.status == TaskStatus::Succeeded => {
                        self.status =
                            format!("Moved {} item(s) to the Recycle Bin.", data.item_count);
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
            } else {
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
                self.status = format!(
                    "{} is a file. Opening files is reserved for a future milestone.",
                    selected_entry.display_name
                );
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

    fn set_todo_status(&mut self, action: CommanderAction) {
        self.status = format!("{action:?} is reserved for a future milestone.");
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

    fn confirm_dialog(&mut self) {
        match &self.dialog {
            DialogState::TextInput(_) => self.handle_text_input(TextInputAction::Submit),
            DialogState::DeleteConfirm(confirm) => {
                let confirm = confirm.clone();
                self.start_delete_task(confirm);
            }
            DialogState::CommandMenu(menu) => {
                let action = menu.selected_action();
                self.dialog = DialogState::None;
                self.dispatch_action(action);
            }
            DialogState::Message { .. } => {
                self.dialog = DialogState::None;
                self.status = "Confirmed placeholder dialog.".to_owned();
            }
            DialogState::None => self.open_or_confirm_selected(),
        }
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
                self.status =
                    "Archive creation is reserved for the compression milestone.".to_owned();
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
        });
        self.status = "Delete confirmation opened.".to_owned();
    }

    fn start_delete_task(&mut self, confirm: DeleteConfirmationState) {
        let paths: Vec<PathBuf> = confirm
            .items
            .iter()
            .map(|entry| PathBuf::from(&entry.virtual_path))
            .collect();
        let label = format!("Deleting {} item(s)", paths.len());
        let task_id = self
            .task_manager
            .spawn_delete_to_recycle_bin(label.clone(), paths);
        self.delete_task_panes.insert(task_id, confirm.pane);
        self.dialog = DialogState::None;
        self.status = format!("{label} in background.");
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
        self.pane_mut(active_pane).current_location = location;
        self.refresh_pane(active_pane);
    }

    fn go_parent(&mut self, side: PaneSide) {
        let parent = fs::parent_location(&self.pane(side).current_location);
        self.pane_mut(side).current_location = parent;
        self.refresh_pane(side);
    }

    fn refresh_pane(&mut self, side: PaneSide) {
        let location = self.pane(side).current_location.clone();
        let previous_selection_path = self
            .pane(side)
            .selected_entry()
            .map(|entry| entry.virtual_path.clone());
        let result = self.local_file_system.list_location(&location);
        let pane = self.pane_mut(side);

        match result {
            Ok(entries) => {
                pane.entries = entries;
                pane.retain_valid_marks();
                pane.restore_selection(previous_selection_path.as_deref());
                pane.load_state = PaneLoadState::Ready;
                self.status = format!("Loaded {}.", fs::location_label(&location));
            }
            Err(error) => {
                let message = error.user_message();
                pane.entries.clear();
                pane.selected_index = 0;
                pane.marked_items.clear();
                pane.scroll_offset = 0.0;
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
        let DialogState::CommandMenu(menu) = &mut self.dialog else {
            return false;
        };

        menu.move_focus(movement);
        true
    }

    pub fn active_operation_sources(&self) -> Vec<FileEntry> {
        self.active_pane().operation_sources()
    }

    pub fn text_dialog_is_open(&self) -> bool {
        matches!(self.dialog, DialogState::TextInput(_))
    }

    fn text_dialog_mut(&mut self) -> Option<&mut TextInputDialogState> {
        match &mut self.dialog {
            DialogState::TextInput(dialog) => Some(dialog),
            DialogState::None
            | DialogState::Message { .. }
            | DialogState::DeleteConfirm(_)
            | DialogState::CommandMenu(_) => None,
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
    pub scroll_offset: f32,
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
            scroll_offset: 0.0,
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
            self.selected_index = 0;
        } else {
            self.selected_index = index.min(self.entries.len() - 1);
        }
        self.follow_selection();
    }

    pub fn move_selection(&mut self, movement: SelectionMovement) {
        if self.entries.is_empty() {
            self.selected_index = 0;
            return;
        }

        let last_index = self.entries.len() - 1;
        self.selected_index = match movement {
            SelectionMovement::Up => self.selected_index.saturating_sub(1),
            SelectionMovement::Down => (self.selected_index + 1).min(last_index),
            SelectionMovement::PageUp => self.selected_index.saturating_sub(PAGE_SIZE),
            SelectionMovement::PageDown => (self.selected_index + PAGE_SIZE).min(last_index),
            SelectionMovement::Home => 0,
            SelectionMovement::End => last_index,
        };
        self.follow_selection();
    }

    pub fn marked_summary(&self) -> MarkedSummary {
        let mut summary = MarkedSummary::default();

        for entry in self.marked_entries() {
            summary.count += 1;
            if let Some(size) = entry.size {
                summary.total_size = summary.total_size.saturating_add(size);
            } else {
                summary.has_unknown_size = true;
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
            self.selected_index = 0;
            self.scroll_offset = 0.0;
            return;
        }

        self.selected_index = previous_selection_path
            .and_then(|path| {
                self.entries
                    .iter()
                    .position(|entry| entry.virtual_path == path)
            })
            .unwrap_or(if self.entries.len() > 1 { 1 } else { 0 });
        self.follow_selection();
    }

    fn follow_selection(&mut self) {
        self.scroll_offset = self.selected_index as f32 * SCROLL_ROW_HEIGHT;
    }
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
    pub fn kind_label(&self) -> &'static str {
        match self.kind {
            FileEntryKind::File => "File",
            FileEntryKind::Directory => "Directory",
            FileEntryKind::Drive => "Drive",
            FileEntryKind::Archive => "Archive",
            FileEntryKind::Parent => "Parent",
            FileEntryKind::FtpItem => "FTP",
            FileEntryKind::Unknown => "Unknown",
        }
    }

    pub fn size_label(&self) -> String {
        match self.size {
            Some(size) => Self::size_value_label(size),
            None => String::new(),
        }
    }

    pub fn size_value_label(size: u64) -> String {
        if size >= 1024 * 1024 {
            format!("{} MB", size / (1024 * 1024))
        } else if size >= 1024 {
            format!("{} KB", size / 1024)
        } else {
            format!("{size} B")
        }
    }

    pub fn modified_label(&self) -> String {
        fs::modified_label(self.modified)
    }

    pub fn flags_label(&self) -> String {
        let mut flags = Vec::new();
        if self.flags.hidden {
            flags.push("H");
        }
        if self.flags.read_only {
            flags.push("R");
        }
        flags.join(" ")
    }

    pub fn is_available_source(&self) -> bool {
        !matches!(self.kind, FileEntryKind::Parent | FileEntryKind::Drive)
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
    CommandMenu(CommandMenuState),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandMenuState {
    pub focused_index: usize,
    pub entries: Vec<CommandMenuEntry>,
}

impl Default for CommandMenuState {
    fn default() -> Self {
        Self {
            focused_index: 0,
            entries: vec![
                CommandMenuEntry::new("Create Folder", CommanderAction::CreateFolder),
                CommandMenuEntry::new("Create File", CommanderAction::CreateFile),
                CommandMenuEntry::new("Create Text File", CommanderAction::CreateTextFile),
                CommandMenuEntry::new("Rename", CommanderAction::Rename),
                CommandMenuEntry::new("Delete", CommanderAction::Delete),
                CommandMenuEntry::new("Task Demo", CommanderAction::StartDemoTask),
                CommandMenuEntry::new("Failing Task Demo", CommanderAction::StartFailingDemoTask),
                CommandMenuEntry::new("Cancel Task", CommanderAction::CancelActiveTask),
            ],
        }
    }
}

impl CommandMenuState {
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
pub struct DeleteConfirmationState {
    pub pane: PaneSide,
    pub items: Vec<FileEntry>,
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
                    KeyboardLayer::Lowercase => KeyboardLayer::Uppercase,
                    KeyboardLayer::Uppercase => KeyboardLayer::Symbols,
                    KeyboardLayer::Symbols => KeyboardLayer::Lowercase,
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
        OnScreenKey::Backspace,
    ],
    &[
        OnScreenKey::Symbols,
        OnScreenKey::CursorLeft,
        OnScreenKey::Space,
        OnScreenKey::CursorRight,
        OnScreenKey::Clear,
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
        OnScreenKey::Backspace,
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
        OnScreenKey::Symbols,
        OnScreenKey::Text("#"),
        OnScreenKey::Text("@"),
        OnScreenKey::Text("+"),
        OnScreenKey::Text("="),
        OnScreenKey::Text("!"),
        OnScreenKey::Text("&"),
        OnScreenKey::Text(";"),
        OnScreenKey::Backspace,
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

fn char_to_byte_index(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(value.len())
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DispatchResult {
    pub quit_requested: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::CommanderAction;

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

    fn test_state() -> AppState {
        AppState {
            settings: Settings::default(),
            left_pane: PaneState {
                title: "Left".to_owned(),
                current_location: fs::ROOTS_LOCATION.to_owned(),
                entries: vec![test_entry("a"), test_entry("b"), test_entry("c")],
                selected_index: 1,
                marked_items: BTreeSet::new(),
                load_state: PaneLoadState::Ready,
                scroll_offset: 0.0,
            },
            right_pane: PaneState {
                title: "Right".to_owned(),
                current_location: fs::ROOTS_LOCATION.to_owned(),
                entries: vec![test_entry("x"), test_entry("y"), test_entry("z")],
                selected_index: 1,
                marked_items: BTreeSet::new(),
                load_state: PaneLoadState::Ready,
                scroll_offset: 0.0,
            },
            active_pane: PaneSide::Left,
            status: String::new(),
            dialog: DialogState::None,
            show_input_help: false,
            local_file_system: LocalFileSystem,
            task_manager: TaskManager::default(),
            delete_task_panes: BTreeMap::new(),
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
    fn selection_movement_is_clamped_to_entries() {
        let mut state = test_state();

        assert_eq!(state.active_pane().selected_index, 1);

        state.dispatch_action(CommanderAction::MoveSelectionUp);
        assert_eq!(state.active_pane().selected_index, 0);

        state.dispatch_action(CommanderAction::MoveSelectionUp);
        assert_eq!(state.active_pane().selected_index, 0);

        state.dispatch_action(CommanderAction::MoveSelectionEnd);
        assert_eq!(
            state.active_pane().selected_index,
            state.active_pane().entries.len() - 1
        );

        state.dispatch_action(CommanderAction::MoveSelectionDown);
        assert_eq!(
            state.active_pane().selected_index,
            state.active_pane().entries.len() - 1
        );

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
    fn restore_selection_keeps_matching_path_when_refreshing_entries() {
        let mut pane = PaneState {
            title: "Test".to_owned(),
            current_location: fs::ROOTS_LOCATION.to_owned(),
            entries: vec![test_entry("a"), test_entry("b"), test_entry("c")],
            selected_index: 1,
            marked_items: BTreeSet::from(["missing".to_owned(), "c".to_owned()]),
            load_state: PaneLoadState::Ready,
            scroll_offset: 0.0,
        };

        pane.entries = vec![test_entry("c"), test_entry("b")];
        pane.retain_valid_marks();
        pane.restore_selection(Some("b"));

        assert_eq!(pane.selected_index, 1);
        assert!(pane.marked_items.contains("c"));
        assert!(!pane.marked_items.contains("missing"));
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
}
