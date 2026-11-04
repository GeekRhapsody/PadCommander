use std::{
    env,
    ffi::CString,
    fs, io,
    mem::MaybeUninit,
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};

pub const ROOTS_LABEL: &str = "Devices";
pub const SUPPORTS_SHORTCUTS: bool = false;

pub fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

pub fn config_dir() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".config")))
        .unwrap_or_else(env::temp_dir)
        .join("PadCommander")
}

/// The file a self-update replaces, and the ending of the release asset that replaces it.
pub fn update_target() -> io::Result<(PathBuf, &'static str)> {
    // Inside an AppImage, current_exe is the binary in the read-only mount; the file to swap is the AppImage itself.
    if let Some(appimage) = env::var_os("APPIMAGE") {
        return Ok((PathBuf::from(appimage), "-linux-x86_64.AppImage"));
    }
    Ok((env::current_exe()?, "-linux-x86_64.tar.gz"))
}

pub fn make_executable(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

pub fn logical_drive_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/")];
    roots.extend(home_dir());

    // Removable media mounts: udisks uses /run/media/$USER (SteamOS SD cards land here),
    // Debian-based distros use /media/$USER, and manual mounts go under /mnt.
    let user = env::var_os("USER").unwrap_or_default();
    for mount_parent in [
        Path::new("/run/media").join(&user),
        Path::new("/media").join(&user),
        PathBuf::from("/mnt"),
    ] {
        let Ok(entries) = fs::read_dir(&mount_parent) else {
            continue;
        };
        roots.extend(
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir()),
        );
    }

    roots
}

pub fn get_volume_label(_root: &Path) -> Option<String> {
    None
}

pub fn delete_to_recycle_bin(paths: &[PathBuf]) -> Result<(), String> {
    trash::delete_all(paths).map_err(|error| format!("Move to Trash failed: {error}"))
}

pub fn open_with_default_app(path: &Path) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("xdg-open failed: {error}"))
}

pub fn create_shortcut(
    _target_path: &Path,
    _shortcut_path: &Path,
    _working_directory: Option<&Path>,
    _icon_path: Option<&Path>,
) -> Result<(), String> {
    Err("Shortcuts are only supported on Windows.".to_owned())
}

pub fn get_disk_free_space(path: &Path) -> Option<u64> {
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stats = MaybeUninit::<libc::statvfs>::uninit();

    // SAFETY: path is a valid NUL-terminated C string and stats points to writable memory
    // large enough for a statvfs struct.
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return None;
    }

    // SAFETY: statvfs returned 0, so it fully initialized stats.
    let stats = unsafe { stats.assume_init() };
    Some(stats.f_bavail.saturating_mul(stats.f_frsize))
}
