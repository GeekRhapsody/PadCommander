use crate::{
    action::{CommanderAction, TextInputAction},
    fs::{self, LocalFileSystem},
    settings::Settings,
    tasks::TaskManager,
};

use std::{collections::BTreeSet, time::SystemTime};

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
        Self {
            settings: Settings::default(),
            left_pane: PaneState::placeholder(
                "Left Pane",
                "C:\\Users\\Player\\Documents",
                &[
                    ("..", FileEntryKind::Parent, None),
                    ("Archives", FileEntryKind::Directory, None),
                    ("Games", FileEntryKind::Directory, None),
                    ("Screenshots", FileEntryKind::Directory, None),
                    ("readme.txt", FileEntryKind::File, Some(4 * 1024)),
                    ("save-slot-01.dat", FileEntryKind::File, Some(128 * 1024)),
                    ("playlist.m3u", FileEntryKind::File, Some(2 * 1024)),
                ],
            ),
            right_pane: PaneState::placeholder(
                "Right Pane",
                "D:\\Backups\\Handheld",
                &[
                    ("..", FileEntryKind::Parent, None),
                    ("Incoming", FileEntryKind::Directory, None),
                    ("Music", FileEntryKind::Directory, None),
                    ("Profiles", FileEntryKind::Directory, None),
                    (
                        "commander.zip",
                        FileEntryKind::Archive,
                        Some(42 * 1024 * 1024),
                    ),
                    ("notes.md", FileEntryKind::File, Some(12 * 1024)),
                    ("theme-preview.png", FileEntryKind::File, Some(840 * 1024)),
                ],
            ),
            active_pane: PaneSide::Left,
            status: "Ready. F1 or View opens input help. Ctrl+Q quits during development."
                .to_owned(),
            dialog: DialogState::None,
            show_input_help: false,
            local_file_system: LocalFileSystem,
            task_manager: TaskManager,
        }
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
                self.status = "GoParent is reserved for filesystem navigation.".to_owned();
            }
            CommanderAction::Refresh => {
                self.status = "Refresh is reserved for filesystem loading.".to_owned();
            }
            CommanderAction::ToggleMarkSelected => self.toggle_mark_selected(),
            CommanderAction::Cancel => {
                if self.show_input_help {
                    self.show_input_help = false;
                    self.status = "Input help closed.".to_owned();
                } else if self.dialog != DialogState::None {
                    self.dialog = DialogState::None;
                    self.status = "Cancelled.".to_owned();
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

        let selected_name = self
            .active_pane()
            .selected_entry()
            .map(|entry| entry.display_name.clone())
            .unwrap_or_else(|| "nothing".to_owned());
        self.status = format!("OpenSelected for {selected_name} is reserved for VFS navigation.");
    }

    fn toggle_mark_selected(&mut self) {
        let Some(path) = self
            .active_pane()
            .selected_entry()
            .map(|entry| entry.virtual_path.clone())
        else {
            self.status = "No entry available to mark.".to_owned();
            return;
        };

        let pane = self.pane_mut(self.active_pane);
        let marked = if pane.marked_items.contains(&path) {
            pane.marked_items.remove(&path);
            false
        } else {
            pane.marked_items.insert(path.clone());
            true
        };

        self.status = if marked {
            format!("Marked {path}.")
        } else {
            format!("Unmarked {path}.")
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
    fn placeholder(
        title: &str,
        location: &str,
        rows: &[(&str, FileEntryKind, Option<u64>)],
    ) -> Self {
        Self {
            title: title.to_owned(),
            current_location: location.to_owned(),
            entries: rows
                .iter()
                .map(|(name, kind, size)| FileEntry {
                    display_name: (*name).to_owned(),
                    virtual_path: format!("{location}\\{name}"),
                    kind: *kind,
                    size: *size,
                    modified: None,
                    flags: FileEntryFlags::default(),
                })
                .collect(),
            selected_index: 1,
            marked_items: BTreeSet::new(),
            load_state: PaneLoadState::Ready,
            scroll_offset: 0.0,
        }
    }

    pub fn selected_entry(&self) -> Option<&FileEntry> {
        self.entries.get(self.selected_index)
    }

    pub fn select_index(&mut self, index: usize) {
        if self.entries.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = index.min(self.entries.len() - 1);
        }
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
            Some(size) if size >= 1024 * 1024 => format!("{} MB", size / (1024 * 1024)),
            Some(size) if size >= 1024 => format!("{} KB", size / 1024),
            Some(size) => format!("{size} B"),
            None => String::new(),
        }
    }

    pub fn modified_label(&self) -> &'static str {
        if self.modified.is_some() {
            "Known"
        } else {
            "-"
        }
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

    #[test]
    fn switching_panes_toggles_active_side() {
        let mut state = AppState::new();

        assert_eq!(state.active_pane, PaneSide::Left);

        state.dispatch_action(CommanderAction::SwitchPane);
        assert_eq!(state.active_pane, PaneSide::Right);

        state.dispatch_action(CommanderAction::SwitchPane);
        assert_eq!(state.active_pane, PaneSide::Left);
    }

    #[test]
    fn selection_movement_is_clamped_to_entries() {
        let mut state = AppState::new();

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
        let mut state = AppState::new();
        let right_initial_selection = state.right_pane.selected_index;

        state.dispatch_action(CommanderAction::MoveSelectionDown);

        assert_eq!(state.left_pane.selected_index, 2);
        assert_eq!(state.right_pane.selected_index, right_initial_selection);
    }
}
