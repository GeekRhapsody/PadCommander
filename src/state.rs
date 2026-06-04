use crate::{
    action::{CommanderAction, TextInputAction},
    fs::{self, LocalFileSystem, VirtualFileSystem},
    settings::Settings,
    tasks::TaskManager,
};

use std::{collections::BTreeSet, time::SystemTime};

const SCROLL_ROW_HEIGHT: f32 = 29.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaneSide {
    Left,
    Right,
}

#[derive(Clone, Debug)]
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
            task_manager: TaskManager,
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
            CommanderAction::MoveSelectionUp => self.move_active_selection(SelectionMovement::Up),
            CommanderAction::MoveSelectionDown => {
                self.move_active_selection(SelectionMovement::Down)
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
            CommanderAction::OpenSelected => self.open_or_confirm_selected(),
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
                } else if self.dialog != DialogState::None {
                    self.dialog = DialogState::None;
                    self.status = "Cancelled.".to_owned();
                } else if self.clear_active_error() {
                    self.status = "Error dismissed.".to_owned();
                } else {
                    self.status = "Cancel/back is reserved for navigation history.".to_owned();
                }
            }
            CommanderAction::Confirm => {
                if self.dialog != DialogState::None {
                    self.dialog = DialogState::None;
                    self.status = "Confirmed placeholder dialog.".to_owned();
                } else {
                    self.open_or_confirm_selected();
                }
            }
            CommanderAction::OpenSettings => {
                self.dialog = DialogState::Message {
                    title: "Settings".to_owned(),
                    message: "Settings are reserved for a future milestone.".to_owned(),
                };
                self.status = "Settings are reserved for a future milestone.".to_owned();
            }
            CommanderAction::CreateFolder => self.begin_text_input("Create Folder"),
            CommanderAction::CreateFile => self.begin_text_input("Create File"),
            CommanderAction::CreateTextFile => self.begin_text_input("Create Text File"),
            CommanderAction::Rename => self.begin_text_input("Rename"),
            CommanderAction::ConnectFtp => self.begin_text_input("Connect FTP"),
            CommanderAction::OpenFavourites => {
                self.status = "Favourites are reserved for a future milestone.".to_owned();
            }
            CommanderAction::OpenCommandPalette => {
                self.status = "Command palette is reserved for a future milestone.".to_owned();
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
            CommanderAction::CopyToOtherPane
            | CommanderAction::MoveToOtherPane
            | CommanderAction::Delete
            | CommanderAction::CompressToOtherPane
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

    pub fn task_summary(&self) -> &'static str {
        self.task_manager.summary()
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
        self.status = match text_action {
            TextInputAction::Begin { purpose, .. } => {
                format!("{purpose:?} text input is reserved for the on-screen keyboard milestone.")
            }
            TextInputAction::Insert(_) => "Text entry is not active yet.".to_owned(),
            TextInputAction::Backspace => "Text entry is not active yet.".to_owned(),
            TextInputAction::Clear => "Text entry is not active yet.".to_owned(),
            TextInputAction::Submit => "Text entry is not active yet.".to_owned(),
        };
    }

    fn begin_text_input(&mut self, title: &str) {
        self.dialog = DialogState::TextInput {
            title: title.to_owned(),
            value: String::new(),
        };
        self.status =
            format!("{title} text input is reserved for the on-screen keyboard milestone.");
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

    pub fn active_operation_sources(&self) -> Vec<FileEntry> {
        self.active_pane().operation_sources()
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

#[derive(Clone, Debug)]
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
    TextInput { title: String, value: String },
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
            task_manager: TaskManager,
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
}
