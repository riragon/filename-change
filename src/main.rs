#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod state;
mod events;
mod preview;
mod rename;
mod widgets;
mod controller;
mod ui;

use druid::{AppLauncher, WindowDesc};
use tracing_subscriber::EnvFilter;
use state::AppState;
use ui::build_ui;

pub fn main() {
    let filter = if let Ok(s) = std::env::var("RUST_LOG") {
        EnvFilter::new(s)
    } else {
        EnvFilter::new("filename_change=debug,druid=warn,druid_shell=off")
    };
    tracing_subscriber::fmt().with_env_filter(filter).with_target(true).init();

    let main_window = WindowDesc::new(build_ui())
        .title("ファイル名一括変更")
        .window_size((900.0, 600.0));
    let initial_state = AppState::new();
    AppLauncher::with_window(main_window)
        .launch(initial_state)
        .expect("Failed to launch application");
}


