#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod config;
mod gui;
mod protocol;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,zbus=warn,tracing=warn")).init();
    let cfg = config::Config::load();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([560.0, 420.0])
            .with_title("lan-keyboard"),
        ..Default::default()
    };
    eframe::run_native(
        "lan-keyboard",
        options,
        Box::new(|_cc| Ok(Box::new(gui::App::new(cfg)))),
    )
}
