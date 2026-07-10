#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod action;
mod app;
mod archive;
mod fs;
mod input;
mod platform;
mod settings;
mod state;
mod tasks;
mod theme;
mod ui;

use app::{APP_TITLE, PadCommanderApp};

fn main() -> eframe::Result {
    let native_options = platform::windows::native_options(APP_TITLE);

    eframe::run_native(
        APP_TITLE,
        native_options,
        Box::new(|cc| Ok(Box::new(PadCommanderApp::new(cc)))),
    )
}
