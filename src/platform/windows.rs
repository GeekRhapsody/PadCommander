use eframe::egui;
use std::path::PathBuf;
use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;

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
