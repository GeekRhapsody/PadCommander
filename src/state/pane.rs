use std::{cmp::Ordering, collections::BTreeSet, path::Path, time::SystemTime};

use super::PaneSortOrder;
use crate::fs;

#[derive(Clone, Debug)]
pub struct PaneState {
    pub current_location: String,
    pub entries: Vec<FileEntry>,
    pub selected_index: usize,
    pub marked_items: BTreeSet<String>,
    pub load_state: PaneLoadState,
    pub sort_order: PaneSortOrder,
    pub scroll_offset: f32,
    pub needs_scroll_to_selected: bool,
    pub cached_free_space: Option<u64>,
    pub(super) navigation_history: Vec<NavigationSnapshot>,
}

impl PaneState {
    pub(super) fn new(location: String) -> Self {
        Self {
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
                if self.selected_index == 0 {
                    last_index
                } else {
                    self.selected_index - 1
                }
            }
            SelectionMovement::Down => {
                if self.selected_index >= last_index {
                    0
                } else {
                    self.selected_index + 1
                }
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

    pub(super) fn retain_valid_marks(&mut self) {
        let valid_paths: BTreeSet<_> = self
            .entries
            .iter()
            .filter(|entry| entry.is_available_source())
            .map(|entry| entry.virtual_path.clone())
            .collect();
        self.marked_items.retain(|path| valid_paths.contains(path));
    }

    pub(super) fn restore_selection(&mut self, previous_selection_path: Option<&str>) {
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

    pub(super) fn reset_selection(&mut self) {
        self.selected_index = 0;
        self.scroll_offset = 0.0;
        self.needs_scroll_to_selected = false;
    }

    pub(super) fn sort_entries(&mut self) {
        let sort_order = self.sort_order;
        self.entries.sort_by(|left, right| {
            file_entry_sort_group(left)
                .cmp(&file_entry_sort_group(right))
                .then_with(|| compare_file_entries(left, right, sort_order))
        });
    }

    pub(super) fn navigation_snapshot(&self) -> NavigationSnapshot {
        NavigationSnapshot {
            location: self.current_location.clone(),
            selected_index: self.selected_index,
            scroll_offset: self.scroll_offset,
        }
    }

    pub(super) fn restore_snapshot_position(&mut self, snapshot: NavigationSnapshot) {
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
pub(super) struct NavigationSnapshot {
    pub(super) location: String,
    pub(super) selected_index: usize,
    pub(super) scroll_offset: f32,
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

impl MarkedSummary {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PaneSummary {
    pub file_count: usize,
    pub folder_count: usize,
    pub total_size: u64,
}

impl PaneSummary {}

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
