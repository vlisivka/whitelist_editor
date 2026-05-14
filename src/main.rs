#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

mod app;
mod mikrotik_data;
mod ssh_client;

use app::WhitelistApp;
use std::sync::Arc;

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Mikrotik Whitelist Editor")
        .with_inner_size([1000.0, 600.0])
        .with_min_inner_size([400.0, 300.0])
        .with_resizable(true);
    let icon_data = eframe::icon_data::from_png_bytes(include_bytes!(
        "../assets/linux/com.github.vlisivka.WhitelistEditor.png"
    ))
    .expect("The icon data must be valid");
    viewport.icon = Some(Arc::new(icon_data));
    viewport.app_id = Some("com.github.vlisivka.WhitelistEditor".to_string());

    let native_options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "MikroTik Whitelist Editor",
        native_options,
        Box::new(|cc| Ok(Box::new(WhitelistApp::new(cc)))),
    )
}
