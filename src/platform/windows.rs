use eframe::egui;

pub fn native_options(title: &str) -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_fullscreen(true)
            .with_decorations(false),
        ..Default::default()
    }
}
