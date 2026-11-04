#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod action;
mod app;
mod archive;
mod fs;
mod ftp;
mod input;
mod platform;
mod secrets;
mod settings;
mod state;
mod tasks;
mod theme;
mod ui;
mod update;

use app::{APP_TITLE, PadCommanderApp};

fn main() -> eframe::Result {
    update::remove_leftover();
    let native_options = platform::native_options(APP_TITLE);

    eframe::run_native(
        APP_TITLE,
        native_options,
        Box::new(|cc| Ok(Box::new(PadCommanderApp::new(cc)))),
    )
}
