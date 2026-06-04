use crate::{
    action::Action,
    fs::{self, LocalFileSystem},
    settings::Settings,
    tasks::TaskManager,
};

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
    pub modal: Option<ModalState>,
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
                    ("..", "Parent", "", ""),
                    ("Archives", "Folder", "", "Today"),
                    ("Games", "Folder", "", "Yesterday"),
                    ("Screenshots", "Folder", "", "Jun 02"),
                    ("readme.txt", "Text", "4 KB", "Jun 01"),
                    ("save-slot-01.dat", "File", "128 KB", "May 30"),
                    ("playlist.m3u", "Media", "2 KB", "May 28"),
                ],
            ),
            right_pane: PaneState::placeholder(
                "Right Pane",
                "D:\\Backups\\Handheld",
                &[
                    ("..", "Parent", "", ""),
                    ("Incoming", "Folder", "", "Today"),
                    ("Music", "Folder", "", "May 31"),
                    ("Profiles", "Folder", "", "May 22"),
                    ("commander.zip", "Archive", "42 MB", "May 20"),
                    ("notes.md", "Text", "12 KB", "May 18"),
                    ("theme-preview.png", "Image", "840 KB", "May 15"),
                ],
            ),
            active_pane: PaneSide::Left,
            status: "Ready. Tab switches panes. Esc or Ctrl+Q quits during development.".to_owned(),
            modal: None,
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

    pub fn apply_action(&mut self, action: Action) {
        match action {
            Action::ActivatePane(side) => {
                self.active_pane = side;
                self.status = format!("{} pane active.", self.active_pane_label());
            }
            Action::SwitchPane => {
                self.active_pane = match self.active_pane {
                    PaneSide::Left => PaneSide::Right,
                    PaneSide::Right => PaneSide::Left,
                };
                self.status = format!("{} pane active.", self.active_pane_label());
            }
            Action::PlaceholderCommand(name) => {
                self.status = format!("{name} is reserved for a future milestone.");
            }
            Action::Quit => {}
        }
    }

    pub fn vfs_label(&self) -> &'static str {
        fs::scheme_name(&self.local_file_system)
    }

    pub fn task_summary(&self) -> &'static str {
        self.task_manager.summary()
    }
}

#[derive(Clone, Debug)]
pub struct PaneState {
    pub title: String,
    pub location: String,
    pub rows: Vec<FileRow>,
    pub selected_row: usize,
}

impl PaneState {
    fn placeholder(title: &str, location: &str, rows: &[(&str, &str, &str, &str)]) -> Self {
        Self {
            title: title.to_owned(),
            location: location.to_owned(),
            rows: rows
                .iter()
                .map(|(name, kind, size, modified)| FileRow {
                    name: (*name).to_owned(),
                    kind: (*kind).to_owned(),
                    size: (*size).to_owned(),
                    modified: (*modified).to_owned(),
                })
                .collect(),
            selected_row: 1,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileRow {
    pub name: String,
    pub kind: String,
    pub size: String,
    pub modified: String,
}

#[derive(Clone, Debug)]
pub struct ModalState {
    pub title: String,
    pub message: String,
}
