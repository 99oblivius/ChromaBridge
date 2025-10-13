// Hide console window on Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod monitors;

#[macro_use]
mod logger;

use anyhow::Result;
use color_interlacer_core::Config;

fn main() -> Result<()> {
    // Load configuration to get log retention setting
    let config = Config::new()?;
    let app_config = config.load().unwrap_or_default();

    // Initialize logger
    let log_dir = config.app_data_dir.join("logs");
    logger::init_logger(log_dir, app_config.log_retention_count)?;

    log_info!("=== Color Interlacer GUI Starting ===");

    // Run the GUI
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 500.0])
            .with_title("Color Interlacer")
            .with_resizable(false),
        ..Default::default()
    };

    let result = eframe::run_native(
        "Color Interlacer",
        options,
        Box::new(|cc| Ok(Box::new(app::ColorInterlacerApp::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!("Failed to run GUI: {}", e));

    // Finalize logs on exit
    log_info!("GUI shutting down...");
    logger::finalize_logs()?;

    result
}
