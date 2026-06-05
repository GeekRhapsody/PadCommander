use eframe::egui;
use std::path::PathBuf;
use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;
use windows_sys::Win32::UI::Shell::{
    FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, SHFILEOPSTRUCTW, SHFileOperationW,
};

pub fn native_options(title: &str) -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_fullscreen(true)
            .with_decorations(false),
        ..Default::default()
    }
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
