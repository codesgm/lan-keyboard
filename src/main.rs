#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
mod capture_linux;
#[cfg(windows)]
mod capture_windows;
mod client;
mod config;
mod gui;
#[cfg(target_os = "linux")]
mod inject_linux;
#[cfg(windows)]
mod inject_windows;
mod keymap;
mod protocol;
mod server;
mod switch;
#[cfg(windows)]
mod win_util;
mod worker;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,zbus=warn,tracing=warn")).init();
    let cfg = config::Config::load();
    if std::env::args().any(|a| a == "--headless") {
        if let Err(e) = worker::run_headless(cfg) {
            log::error!("{e:#}");
            std::process::exit(1);
        }
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([560.0, 420.0])
            .with_title("lan-keyboard"),
        ..Default::default()
    };
    eframe::run_native(
        "lan-keyboard",
        options,
        Box::new(|cc| Ok(Box::new(gui::App::new(cfg, cc.egui_ctx.clone())))),
    )
}
