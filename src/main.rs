mod action;
mod app;
mod fs;
mod input;
mod platform;
mod settings;
mod state;
mod tasks;
mod ui;

use app::{APP_TITLE, HandheldCommanderApp};

fn main() -> eframe::Result {
    let native_options = platform::windows::native_options(APP_TITLE);

    eframe::run_native(
        APP_TITLE,
        native_options,
        Box::new(|cc| Ok(Box::new(HandheldCommanderApp::new(cc)))),
    )
}
