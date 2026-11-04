use std::env;
use std::io;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Storage::FileSystem::{GetLogicalDrives, GetVolumeInformationW};
use windows_sys::Win32::UI::Shell::{
    FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, SHFILEOPSTRUCTW, SHFileOperationW,
};

pub const ROOTS_LABEL: &str = "This PC";
pub const SUPPORTS_SHORTCUTS: bool = true;

pub fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE").map(PathBuf::from).or_else(|| {
        let drive = env::var_os("HOMEDRIVE")?;
        let path = env::var_os("HOMEPATH")?;
        let mut home = PathBuf::from(drive);
        home.push(path);
        Some(home)
    })
}

pub fn config_dir() -> PathBuf {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("PadCommander")
}

/// The file a self-update replaces, and the ending of the release asset that replaces it.
pub fn update_target() -> io::Result<(PathBuf, &'static str)> {
    Ok((env::current_exe()?, "-windows-x86_64.zip"))
}

pub fn make_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

pub fn logical_drive_roots() -> Vec<PathBuf> {
    // SAFETY: GetLogicalDrives takes no pointers and has no caller-side preconditions.
    let drive_mask = unsafe { GetLogicalDrives() };
    let mut roots = Vec::new();

    for index in 0..26 {
        if drive_mask & (1 << index) != 0 {
            let letter = (b'A' + index as u8) as char;
            roots.push(PathBuf::from(format!("{letter}:\\")));
        }
    }

    roots
}

pub fn get_volume_label(root: &std::path::Path) -> Option<String> {
    let root_wide: Vec<u16> = {
        use std::os::windows::ffi::OsStrExt;
        root.as_os_str().encode_wide().chain([0]).collect()
    };
    let mut name_buf = vec![0u16; 256];

    // SAFETY: root_wide is a valid NUL-terminated UTF-16 path; name_buf is a valid output buffer.
    let ok = unsafe {
        GetVolumeInformationW(
            root_wide.as_ptr(),
            name_buf.as_mut_ptr(),
            name_buf.len() as u32,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        )
    };

    if ok == 0 {
        return None;
    }

    let len = name_buf
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(name_buf.len());
    if len == 0 {
        return None;
    }

    String::from_utf16(&name_buf[..len]).ok()
}

pub fn delete_to_recycle_bin(paths: &[PathBuf]) -> Result<(), String> {
    let mut from = Vec::new();
    for path in paths {
        let mut wide: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().collect();
        from.append(&mut wide);
        from.push(0);
    }
    from.push(0);

    let mut operation = SHFILEOPSTRUCTW {
        hwnd: std::ptr::null_mut(),
        wFunc: FO_DELETE,
        pFrom: from.as_ptr(),
        pTo: std::ptr::null(),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI) as u16,
        fAnyOperationsAborted: 0,
        hNameMappings: std::ptr::null_mut(),
        lpszProgressTitle: std::ptr::null(),
    };

    // SAFETY: SHFileOperationW receives a valid, double-NUL-terminated UTF-16 path list
    // that remains alive for the duration of the call, and all optional pointers are null.
    let result = unsafe { SHFileOperationW(&mut operation) };
    if result != 0 {
        return Err(format!("Recycle Bin delete failed with code {result}."));
    }
    if operation.fAnyOperationsAborted != 0 {
        return Err("Recycle Bin delete was aborted.".to_owned());
    }

    Ok(())
}

pub fn open_with_default_app(path: &std::path::Path) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;

    let mut file: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().collect();
    file.push(0);
    let operation: Vec<u16> = "open\0".encode_utf16().collect();

    // SAFETY: ShellExecuteW receives valid null-terminated UTF-16 strings.
    // hwnd is null (no owner window), parameters and directory are null.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1, // SW_SHOWNORMAL
        )
    };

    if result as usize > 32 {
        Ok(())
    } else {
        Err(format!("ShellExecuteW failed with code {:?}.", result))
    }
}

pub fn create_shortcut(
    target_path: &std::path::Path,
    shortcut_path: &std::path::Path,
    working_directory: Option<&std::path::Path>,
    icon_path: Option<&std::path::Path>,
) -> Result<(), String> {
    use windows::{
        Win32::{
            System::Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize, IPersistFile,
            },
            UI::Shell::{IShellLinkW, ShellLink},
        },
        core::{Interface, PCWSTR},
    };

    struct ComApartment;

    impl ComApartment {
        fn init() -> Result<Self, String> {
            // SAFETY: Initializing COM for this thread with no reserved pointer.
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
                .ok()
                .map_err(|error| format!("Cannot initialize COM: {error}"))?;
            Ok(Self)
        }
    }

    impl Drop for ComApartment {
        fn drop(&mut self) {
            // SAFETY: Balances a successful CoInitializeEx call on this thread.
            unsafe { CoUninitialize() };
        }
    }

    let _com = ComApartment::init()?;
    let target = wide_null(target_path);
    let shortcut = wide_null(shortcut_path);

    // SAFETY: CoCreateInstance is called after COM initialization and returns a typed interface.
    let shell_link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
            .map_err(|error| format!("Cannot create Shell Link COM object: {error}"))?;

    // SAFETY: All PCWSTR values point to valid, NUL-terminated UTF-16 buffers alive for the call.
    unsafe {
        shell_link
            .SetPath(PCWSTR(target.as_ptr()))
            .map_err(|error| format!("Cannot set shortcut target: {error}"))?;

        if let Some(working_directory) = working_directory {
            let working_directory = wide_null(working_directory);
            shell_link
                .SetWorkingDirectory(PCWSTR(working_directory.as_ptr()))
                .map_err(|error| format!("Cannot set shortcut working directory: {error}"))?;
        }

        if let Some(icon_path) = icon_path {
            let icon_path = wide_null(icon_path);
            shell_link
                .SetIconLocation(PCWSTR(icon_path.as_ptr()), 0)
                .map_err(|error| format!("Cannot set shortcut icon: {error}"))?;
        }
    }

    let persist_file: IPersistFile = shell_link
        .cast()
        .map_err(|error| format!("Cannot access shortcut persistence API: {error}"))?;
    // SAFETY: The shortcut path buffer is valid and NUL-terminated; true allows overwrite.
    unsafe { persist_file.Save(PCWSTR(shortcut.as_ptr()), true) }
        .map_err(|error| format!("Cannot save shortcut {}: {error}", shortcut_path.display()))
}

fn wide_null(path: &std::path::Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str().encode_wide().chain([0]).collect()
}

pub fn get_disk_free_space(path: &std::path::Path) -> Option<u64> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let path_wide: Vec<u16> = wide_null(path);
    let mut free_bytes = 0u64;

    // SAFETY: GetDiskFreeSpaceExW receives a valid, NUL-terminated UTF-16 path
    // and we pass valid pointers for the output parameter.
    let result = unsafe {
        GetDiskFreeSpaceExW(
            path_wide.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut free_bytes,
        )
    };

    if result != 0 { Some(free_bytes) } else { None }
}
