use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;
use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;
use windows_sys::Win32::UI::Shell::{
    FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, SHFILEOPSTRUCTW, SHFileOperationW,
};

pub fn native_options(title: &str) -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_icon(app_icon())
            .with_fullscreen(true)
            .with_decorations(false),
        ..Default::default()
    }
}

fn app_icon() -> Arc<egui::IconData> {
    Arc::new(decode_app_icon().unwrap_or_default())
}

fn decode_app_icon() -> Result<egui::IconData, image::ImageError> {
    let image = image::load_from_memory_with_format(
        include_bytes!("../../assets/windows/app.ico"),
        image::ImageFormat::Ico,
    )?
    .into_rgba8();
    let (width, height) = image.dimensions();

    Ok(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
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

#[cfg(windows)]
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

#[cfg(not(windows))]
pub fn delete_to_recycle_bin(_paths: &[PathBuf]) -> Result<(), String> {
    Err("Recycle Bin delete is only supported on Windows.".to_owned())
}

#[cfg(windows)]
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

#[cfg(windows)]
fn wide_null(path: &std::path::Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str().encode_wide().chain([0]).collect()
}

#[cfg(not(windows))]
pub fn create_shortcut(
    _target_path: &std::path::Path,
    _shortcut_path: &std::path::Path,
    _working_directory: Option<&std::path::Path>,
    _icon_path: Option<&std::path::Path>,
) -> Result<(), String> {
    Err("Windows shortcuts are only supported on Windows.".to_owned())
}

#[cfg(windows)]
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

    if result != 0 {
        Some(free_bytes)
    } else {
        None
    }
}

#[cfg(not(windows))]
pub fn get_disk_free_space(_path: &std::path::Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_app_icon_decodes_for_window_icon() {
        let icon = decode_app_icon().expect("decode app icon");

        assert!(!icon.rgba.is_empty());
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
    }
}
