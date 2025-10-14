// Hide console window on Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod monitors;
mod ipc_client;

use anyhow::Result;
use color_interlacer_core::{Config, log_info, log_warn};

fn main() -> Result<()> {
    // Load configuration to get log retention setting
    let config = Config::new()?;
    let app_config = config.load().unwrap_or_default();

    // Initialize logger
    let log_dir = config.app_data_dir.join("logs");
    color_interlacer_core::logger::init_logger(log_dir, "gui", app_config.log_retention_count)?;

    log_info!("=== Color Interlacer GUI Starting ===");

    // Load window icon
    let icon_data = {
        let icon_path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .map(|p| p.join("assets").join("icons").join("icon-2048.png"));

        if let Some(path) = icon_path {
            if path.exists() {
                match image::open(&path) {
                    Ok(img) => {
                        let rgba = img.to_rgba8();
                        let (width, height) = rgba.dimensions();
                        Some(egui::IconData {
                            rgba: rgba.into_raw(),
                            width,
                            height,
                        })
                    }
                    Err(e) => {
                        log_warn!("Failed to load window icon from {:?}: {}", path, e);
                        None
                    }
                }
            } else {
                log_warn!("Window icon not found at {:?}", path);
                None
            }
        } else {
            None
        }
    };

    // Run the GUI
    let mut viewport_builder = egui::ViewportBuilder::default()
        .with_inner_size([400.0, 500.0])
        .with_title("Color Interlacer")
        .with_resizable(false)
        .with_maximize_button(false);

    if let Some(icon) = icon_data {
        viewport_builder = viewport_builder.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport: viewport_builder,
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
    color_interlacer_core::logger::finalize_logs()?;

    result
}
