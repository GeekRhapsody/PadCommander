use std::path::{Path, PathBuf};

use super::{FileEntry, PaneSide, PaneSortOrder, SelectionMovement, TextInputDialogState};
use crate::{
    action::CommanderAction,
    tasks::{TaskId, TaskKind},
};

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
    Favourites(ListModalState),
    FtpServers(ListModalState),
    SavePassword(SavePasswordState),
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
        const LAST_INDEX: usize = 5;

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
            3 => SettingsModalFocus::ControllerOverride,
            4 => SettingsModalFocus::Theme,
            5 => SettingsModalFocus::Quit,
            _ => SettingsModalFocus::UiScale,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsModalFocus {
    UiScale,
    UseRecycleBin,
    ShowHiddenFiles,
    ControllerOverride,
    Theme,
    Quit,
}

/// Focus for a vertical list dialog (Favourites, Servers).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ListModalState {
    pub focused_index: usize,
}

impl ListModalState {
    pub fn move_focus(&mut self, movement: SelectionMovement, row_count: usize) {
        if row_count == 0 {
            self.focused_index = 0;
            return;
        }

        let last_index = row_count - 1;
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

    pub fn clamp_focus(&mut self, row_count: usize) {
        if row_count == 0 {
            self.focused_index = 0;
        } else {
            self.focused_index = self.focused_index.min(row_count - 1);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavePasswordState {
    pub side: PaneSide,
    pub server_label: String,
    pub focused_index: usize,
}

impl SavePasswordState {
    pub fn move_focus(&mut self, movement: SelectionMovement) {
        self.focused_index = match movement {
            SelectionMovement::Up | SelectionMovement::PageUp | SelectionMovement::Home => 0,
            SelectionMovement::Down | SelectionMovement::PageDown | SelectionMovement::End => 1,
        };
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

    pub(super) fn selected_request(&self) -> DecompressRequest {
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
pub(super) struct DecompressRequest {
    pub(super) archive: FileEntry,
    pub(super) archive_stem: String,
    pub(super) target_side: PaneSide,
    pub(super) create_archive_folder: bool,
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
        Self::new(false, true, false)
    }
}

impl CommandMenuState {
    pub fn new(include_create_shortcut: bool, include_archives: bool, on_ftp_pane: bool) -> Self {
        // Grouped as the actions sheet shows them: transfer, archive, organize, remote, remove.
        let mut entries = vec![
            CommandMenuEntry::new("Copy", CommanderAction::CopyToOtherPane),
            CommandMenuEntry::new("Move", CommanderAction::MoveToOtherPane),
        ];

        if include_create_shortcut {
            entries.push(CommandMenuEntry::new(
                "Create Shortcut",
                CommanderAction::CreateShortcutInOtherPane,
            ));
        }

        if include_archives {
            entries.extend([
                CommandMenuEntry::new("Compress", CommanderAction::CompressToOtherPane),
                CommandMenuEntry::new("Decompress", CommanderAction::DecompressToOtherPane),
            ]);
        }

        entries.extend([
            CommandMenuEntry::new("Create Folder", CommanderAction::CreateFolder),
            CommandMenuEntry::new("Create Text File", CommanderAction::CreateTextFile),
            CommandMenuEntry::new("Rename", CommanderAction::Rename),
            CommandMenuEntry::new("Sort By", CommanderAction::OpenSortBy),
            if on_ftp_pane {
                CommandMenuEntry::new("Disconnect", CommanderAction::DisconnectFtp)
            } else {
                CommandMenuEntry::new("Connect to Server", CommanderAction::ConnectFtp)
            },
            CommandMenuEntry::new("Delete", CommanderAction::Delete),
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
                if self.focused_index == 0 {
                    last_index
                } else {
                    self.focused_index - 1
                }
            }
            SelectionMovement::Down | SelectionMovement::PageDown => {
                if self.focused_index >= last_index {
                    0
                } else {
                    self.focused_index + 1
                }
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
    pub(super) request: ShortcutRequest,
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
pub(super) struct PendingShortcutRequest {
    pub(super) target_path: PathBuf,
    pub(super) target_directory: PathBuf,
    pub(super) working_directory: Option<PathBuf>,
    pub(super) icon_path: Option<PathBuf>,
    pub(super) target_side: PaneSide,
}

impl PendingShortcutRequest {
    pub(super) fn default_shortcut_name(&self) -> String {
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

    pub(super) fn into_shortcut_request(self, shortcut_name: String) -> ShortcutRequest {
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
pub(super) struct ShortcutRequest {
    pub(super) target_path: PathBuf,
    pub(super) shortcut_path: PathBuf,
    pub(super) working_directory: Option<PathBuf>,
    pub(super) icon_path: Option<PathBuf>,
    pub(super) target_side: PaneSide,
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
    pub sources: Vec<FileEntry>,
    pub target_location: String,
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
