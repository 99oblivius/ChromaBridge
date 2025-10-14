// Hide console window on Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod monitor;
mod window_flags;
mod dcomp_overlay;

use anyhow::Result;
use color_interlacer_core::{Config, HueMapper, NoiseTexture, SpectrumPair, log_info, log_error};
use std::sync::Arc;
use parking_lot::RwLock;

pub struct OverlayState {
    pub spectrum_pair: SpectrumPair,
    pub noise_texture: Option<NoiseTexture>,
    pub hue_mapper: HueMapper,
    pub monitor_index: usize,
    pub fps: f32,
    pub frame_time_ms: f32,
}

fn main() -> Result<()> {
    let config = Config::new()?;

    // Initialize logger
    let log_dir = config.app_data_dir.join("logs");
    let app_config = config.load().unwrap_or_default();
    color_interlacer_core::logger::init_logger(log_dir, "overlay", app_config.log_retention_count)?;

    log_info!("=== Overlay Session Started ===");

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let mut monitor_index = 0;
    let mut spectrum_name = String::new();
    let mut noise_name: Option<String> = None;
    let mut strength = 1.0;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--monitor" => {
                if i + 1 < args.len() {
                    monitor_index = args[i + 1].parse().unwrap_or(0);
                    i += 1;
                }
            }
            "--spectrum" => {
                if i + 1 < args.len() {
                    spectrum_name = args[i + 1].clone();
                    i += 1;
                }
            }
            "--noise" => {
                if i + 1 < args.len() {
                    noise_name = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--strength" => {
                if i + 1 < args.len() {
                    strength = args[i + 1].parse().unwrap_or(1.0);
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    log_info!("Configuration: monitor={}, spectrum={}, noise={:?}, strength={}",
             monitor_index, spectrum_name, noise_name, strength);

    // Load spectrum
    let spectrum_path = config.get_spectrum_path(&spectrum_name);
    let spectrum_pair = match SpectrumPair::load_from_file(spectrum_path) {
        Ok(sp) => {
            log_info!("Loaded spectrum: {}", spectrum_name);
            sp
        }
        Err(e) => {
            log_error!("Failed to load spectrum '{}': {}", spectrum_name, e);
            return Err(e);
        }
    };

    // Load noise texture if specified
    let noise_texture = if let Some(ref name) = noise_name {
        let noise_path = config.get_noise_path(name);
        match NoiseTexture::load_from_file(noise_path) {
            Ok(nt) => {
                log_info!("Loaded noise texture: {}", name);
                Some(nt)
            }
            Err(e) => {
                log_error!("Failed to load noise texture '{}': {}", name, e);
                None
            }
        }
    } else {
        None
    };

    // Create hue mapper with command line strength
    let hue_mapper = HueMapper::new(strength);

    // Create overlay state
    let state = Arc::new(RwLock::new(OverlayState {
        spectrum_pair,
        noise_texture,
        hue_mapper,
        monitor_index,
        fps: 0.0,
        frame_time_ms: 0.0,
    }));

    // Run DirectComposition overlay (like Xbox Game Bar)
    log_info!("Starting DirectComposition overlay...");
    let mut overlay = dcomp_overlay::DCompOverlay::new(state)?;
    let result = overlay.run_message_loop();

    // Finalize logs on exit
    log_info!("Overlay shutting down...");
    color_interlacer_core::logger::finalize_logs()?;

    result
}
