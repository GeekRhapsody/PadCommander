#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(windows)]
pub use windows::*;

use eframe::egui;
use std::sync::Arc;

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
