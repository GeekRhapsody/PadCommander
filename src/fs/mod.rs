use crate::{
    platform::windows,
    state::{FileEntry, FileEntryFlags, FileEntryKind},
};

use chrono::{DateTime, Local};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

pub const ROOTS_LOCATION: &str = "";

#[derive(Clone, Debug, Default)]
pub struct LocalFileSystem;

pub trait VirtualFileSystem {
    fn scheme(&self) -> &'static str;
    fn initial_location(&self) -> String;
    fn list_location(&self, location: &str) -> Result<Vec<FileEntry>, VfsError>;
}

impl VirtualFileSystem for LocalFileSystem {
    fn scheme(&self) -> &'static str {
        "local"
    }

    fn initial_location(&self) -> String {
        user_profile_location().unwrap_or_else(|| ROOTS_LOCATION.to_owned())
    }

    fn list_location(&self, location: &str) -> Result<Vec<FileEntry>, VfsError> {
        if location == ROOTS_LOCATION {
            return Ok(list_drive_roots());
        }

        list_directory(Path::new(location))
    }
}

pub fn scheme_name(vfs: &dyn VirtualFileSystem) -> &'static str {
    vfs.scheme()
}

pub fn location_label(location: &str) -> &str {
    if location == ROOTS_LOCATION {
        "This PC"
    } else {
        location
    }
}

pub fn parent_location(location: &str) -> String {
    if location == ROOTS_LOCATION {
        return ROOTS_LOCATION.to_owned();
    }

    let path = Path::new(location);
    path.parent()
        .map(path_to_location)
        .unwrap_or_else(|| ROOTS_LOCATION.to_owned())
}

fn user_profile_location() -> Option<String> {
    env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .or_else(|| {
            let drive = env::var_os("HOMEDRIVE")?;
            let path = env::var_os("HOMEPATH")?;
            let mut home = PathBuf::from(drive);
            home.push(path);
            Some(home)
        })
        .filter(|path| path.is_dir())
        .map(|path| path_to_location(&path))
}

fn list_drive_roots() -> Vec<FileEntry> {
    windows::logical_drive_roots()
        .into_iter()
        .map(|root| {
            let display_name = root.to_string_lossy().to_string();
            FileEntry {
                display_name: display_name.clone(),
                virtual_path: path_to_location(&root),
                kind: FileEntryKind::Drive,
                size: None,
                modified: None,
                flags: FileEntryFlags::default(),
            }
        })
        .collect()
}

fn list_directory(path: &Path) -> Result<Vec<FileEntry>, VfsError> {
    let mut entries = Vec::new();

    if let Some(parent) = path.parent() {
        entries.push(FileEntry {
            display_name: "..".to_owned(),
            virtual_path: path_to_location(parent),
            kind: FileEntryKind::Parent,
            size: None,
            modified: None,
            flags: FileEntryFlags::default(),
        });
    } else {
        entries.push(FileEntry {
            display_name: "..".to_owned(),
            virtual_path: ROOTS_LOCATION.to_owned(),
            kind: FileEntryKind::Parent,
            size: None,
            modified: None,
            flags: FileEntryFlags::default(),
        });
    }

    for entry in fs::read_dir(path).map_err(|source| VfsError::ReadDirectory {
        location: path_to_location(path),
        source,
    })? {
        let entry = entry.map_err(|source| VfsError::ReadDirectoryEntry {
            location: path_to_location(path),
            source,
        })?;
        let metadata = entry.metadata().ok();
        let file_type = metadata
            .as_ref()
            .map(|metadata| entry_kind_from_path_metadata(&entry.path(), metadata))
            .unwrap_or(FileEntryKind::Unknown);
        let size = metadata
            .as_ref()
            .filter(|_| matches!(file_type, FileEntryKind::File | FileEntryKind::Archive))
            .map(|metadata| metadata.len());
        let modified = metadata
            .as_ref()
            .and_then(|metadata| metadata.modified().ok());
        let flags = metadata
            .as_ref()
            .map(flags_from_metadata)
            .unwrap_or_default();

        entries.push(FileEntry {
            display_name: entry.file_name().to_string_lossy().to_string(),
            virtual_path: path_to_location(&entry.path()),
            kind: file_type,
            size,
            modified,
            flags,
        });
    }

    sort_entries(&mut entries);
    Ok(entries)
}

fn entry_kind_from_path_metadata(path: &Path, metadata: &fs::Metadata) -> FileEntryKind {
    if metadata.is_dir() {
        FileEntryKind::Directory
    } else if is_archive_path(path) {
        FileEntryKind::Archive
    } else if metadata.is_file() {
        FileEntryKind::File
    } else {
        FileEntryKind::Unknown
    }
}

fn is_archive_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "7z" | "bz2" | "gz" | "rar" | "tar" | "xz" | "zip"
            )
        })
        .unwrap_or(false)
}

fn flags_from_metadata(metadata: &fs::Metadata) -> FileEntryFlags {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_READONLY: u32 = 0x0000_0001;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x0000_0002;

        let attributes = metadata.file_attributes();
        FileEntryFlags {
            hidden: attributes & FILE_ATTRIBUTE_HIDDEN != 0,
            read_only: attributes & FILE_ATTRIBUTE_READONLY != 0,
        }
    }

    #[cfg(not(windows))]
    {
        FileEntryFlags {
            hidden: false,
            read_only: metadata.permissions().readonly(),
        }
    }
}

fn sort_entries(entries: &mut [FileEntry]) {
    entries.sort_by(|left, right| {
        entry_sort_group(left)
            .cmp(&entry_sort_group(right))
            .then_with(|| {
                left.display_name
                    .to_lowercase()
                    .cmp(&right.display_name.to_lowercase())
            })
    });
}

fn entry_sort_group(entry: &FileEntry) -> u8 {
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

fn path_to_location(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

#[derive(Debug)]
pub enum VfsError {
    ReadDirectory { location: String, source: io::Error },
    ReadDirectoryEntry { location: String, source: io::Error },
}

impl VfsError {
    pub fn user_message(&self) -> String {
        match self {
            VfsError::ReadDirectory { location, source }
            | VfsError::ReadDirectoryEntry { location, source } => {
                format!("Cannot read {}: {source}", location_label(location))
            }
        }
    }
}

pub fn modified_label(modified: Option<SystemTime>) -> String {
    let Some(modified) = modified else {
        return "-".to_owned();
    };

    DateTime::<Local>::from(modified)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, kind: FileEntryKind) -> FileEntry {
        FileEntry {
            display_name: name.to_owned(),
            virtual_path: name.to_owned(),
            kind,
            size: None,
            modified: None,
            flags: FileEntryFlags::default(),
        }
    }

    #[test]
    fn sort_places_parent_then_directories_then_files_case_insensitive() {
        let mut entries = vec![
            entry("z-file.txt", FileEntryKind::File),
            entry("beta", FileEntryKind::Directory),
            entry("Alpha.txt", FileEntryKind::File),
            entry("..", FileEntryKind::Parent),
            entry("alpha", FileEntryKind::Directory),
        ];

        sort_entries(&mut entries);

        let names: Vec<_> = entries
            .iter()
            .map(|entry| entry.display_name.as_str())
            .collect();
        assert_eq!(names, ["..", "alpha", "beta", "Alpha.txt", "z-file.txt"]);
    }
}
