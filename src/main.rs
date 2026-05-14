mod app;
mod mikrotik_data;
mod ssh_client;

use app::WhitelistApp;

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 600.0])
            .with_min_inner_size([400.0, 300.0]),
        ..Default::default()
    };

    eframe::run_native(
        "MikroTik Whitelist Editor",
        native_options,
        Box::new(|cc| Ok(Box::new(WhitelistApp::new(cc)))),
    )
}
