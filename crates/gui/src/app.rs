use color_interlacer_core::DbConfig;
use crate::monitors::{enumerate_monitors, MonitorInfo};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

pub struct ColorInterlacerApp {
    config: DbConfig,

    // UI state
    overlay_running: bool,
    overlay_process: Option<Child>,
    show_advanced: bool,

    // Monitor selection
    monitors: Vec<MonitorInfo>,
    selected_monitor: usize,

    // Colorblind settings
    spectrum_files: Vec<String>,
    selected_spectrum: Option<usize>,

    noise_files: Vec<String>,
    selected_noise: Option<usize>,

    strength: f32,

    // Debouncing for strength slider
    strength_changed: bool,
    strength_last_change: Instant,

    // FPS tracking
    fps: f32,
    frame_time_ms: f32,
    last_fps_update: Instant,

    // Status message
    status_message: Option<String>,
}

impl ColorInterlacerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let config = DbConfig::new().expect("Failed to initialize database config");

        // Migrate from JSON if exists
        if let Err(e) = config.migrate_from_json() {
            eprintln!("Warning: Failed to migrate from JSON: {}", e);
        }

        let monitors = enumerate_monitors().unwrap_or_default();
        let selected_monitor = config.get_last_monitor().unwrap_or(0).min(monitors.len().saturating_sub(1));

        let spectrum_files = config.list_spectrum_files().unwrap_or_default();
        let selected_spectrum = config.get_colorblind_type().and_then(|name| {
            spectrum_files.iter().position(|s| s == &name)
        });

        let noise_files = config.list_noise_files().unwrap_or_default();
        let selected_noise = config.get_noise_texture().and_then(|name| {
            noise_files.iter().position(|n| n == &name)
        });

        let strength = config.get_strength();

        Self {
            config,
            overlay_running: false,
            overlay_process: None,
            show_advanced: false,
            monitors,
            selected_monitor,
            spectrum_files,
            selected_spectrum,
            noise_files,
            selected_noise,
            strength,
            strength_changed: false,
            strength_last_change: Instant::now(),
            fps: 0.0,
            frame_time_ms: 0.0,
            last_fps_update: Instant::now(),
            status_message: None,
        }
    }

    /// Restart overlay with current settings if it's running
    fn restart_overlay_if_running(&mut self) {
        if self.overlay_running {
            self.stop_overlay();
            // Small delay to ensure clean shutdown
            std::thread::sleep(Duration::from_millis(100));
            self.start_overlay();
        }
    }

    fn start_overlay(&mut self) {
        if self.overlay_running {
            return;
        }

        // Validate and get spectrum name
        let spectrum_name = match self.selected_spectrum {
            Some(idx) if idx < self.spectrum_files.len() => {
                let name = &self.spectrum_files[idx];

                // Validate the spectrum file before starting
                if !self.config.validate_spectrum_file(name) {
                    crate::log_info!("Spectrum file '{}' is invalid, removing from list", name);
                    self.status_message = Some(format!("Spectrum '{}' is invalid and has been removed", name));

                    // Refresh to remove invalid files
                    self.refresh_assets();
                    return;
                }

                name
            }
            _ => {
                self.status_message = Some("Please select a valid spectrum".to_string());
                return;
            }
        };

        // Validate noise texture if selected
        if let Some(noise_idx) = self.selected_noise {
            if noise_idx < self.noise_files.len() {
                let noise_name = &self.noise_files[noise_idx];

                if !self.config.validate_noise_file(noise_name) {
                    crate::log_info!("Noise file '{}' is invalid, removing from list", noise_name);
                    self.status_message = Some(format!("Noise texture '{}' is invalid and has been removed", noise_name));

                    // Clear noise selection and refresh
                    self.selected_noise = None;
                    self.refresh_assets();
                    return;
                }
            }
        }

        // Build overlay executable path
        let exe_path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .map(|p| p.join("color-interlacer-overlay.exe"))
            .expect("Failed to determine overlay executable path");

        // Build command line arguments
        let mut cmd = Command::new(&exe_path);
        cmd.arg("--monitor").arg(self.selected_monitor.to_string());
        cmd.arg("--spectrum").arg(spectrum_name);
        cmd.arg("--strength").arg(self.strength.to_string());

        if let Some(noise_idx) = self.selected_noise {
            cmd.arg("--noise").arg(&self.noise_files[noise_idx]);
        }

        match cmd.spawn() {
            Ok(child) => {
                crate::log_info!("Overlay started: monitor={}, spectrum={}, strength={}",
                                self.selected_monitor, spectrum_name, self.strength);
                self.overlay_process = Some(child);
                self.overlay_running = true;
                self.status_message = Some("Overlay started".to_string());

                // Save config (async writes to database)
                self.config.set_last_monitor(Some(self.selected_monitor));
                self.config.set_colorblind_type(Some(spectrum_name.clone()));
                self.config.set_noise_texture(self.selected_noise.map(|i| self.noise_files[i].clone()));
                self.config.set_strength(self.strength);
                self.config.set_overlay_enabled(true);
            }
            Err(e) => {
                crate::log_info!("Failed to start overlay: {}", e);
                self.status_message = Some(format!("Failed to start overlay: {}", e));
            }
        }
    }

    fn stop_overlay(&mut self) {
        if let Some(mut child) = self.overlay_process.take() {
            crate::log_info!("Stopping overlay process");
            let _ = child.kill();
            let _ = child.wait();
        }

        self.overlay_running = false;
        self.status_message = Some("Overlay stopped".to_string());

        self.config.set_overlay_enabled(false);
    }

    fn open_asset_folder(&self) {
        #[cfg(windows)]
        {
            let _ = Command::new("explorer")
                .arg(self.config.assets_dir().to_str().unwrap_or(""))
                .spawn();
        }
    }

    fn refresh_assets(&mut self) {
        // Reload spectrum files
        self.spectrum_files = self.config.list_spectrum_files().unwrap_or_default();

        // Reload noise files
        self.noise_files = self.config.list_noise_files().unwrap_or_default();

        // Revalidate selections
        if let Some(idx) = self.selected_spectrum {
            if idx >= self.spectrum_files.len() {
                self.selected_spectrum = None;
            }
        }

        if let Some(idx) = self.selected_noise {
            if idx >= self.noise_files.len() {
                self.selected_noise = None;
            }
        }

        // Try to restore from config if selections were cleared
        if self.selected_spectrum.is_none() {
            if let Some(name) = self.config.get_colorblind_type() {
                self.selected_spectrum = self.spectrum_files.iter().position(|s| s == &name);
            }
        }

        if self.selected_noise.is_none() {
            if let Some(name) = self.config.get_noise_texture() {
                self.selected_noise = self.noise_files.iter().position(|n| n == &name);
            }
        }

        self.status_message = Some(format!(
            "Refreshed: {} spectrums, {} noise textures",
            self.spectrum_files.len(),
            self.noise_files.len()
        ));
    }

    fn draw_interface(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Color Interlacer");
                ui.add_space(15.0);

                // Main controls in horizontal layout
                ui.horizontal(|ui| {
                    // Start/Stop button
                    let button_text = if self.overlay_running { "Stop Overlay" } else { "Start Overlay" };
                    let button = egui::Button::new(button_text)
                        .min_size(egui::vec2(120.0, 30.0));
                    if ui.add(button).clicked() {
                        if self.overlay_running {
                            self.stop_overlay();
                        } else {
                            self.start_overlay();
                        }
                    }

                    ui.add_space(20.0);

                    // FPS display (only when overlay is running)
                    if self.overlay_running {
                        ui.vertical(|ui| {
                            ui.label(format!("FPS: {:.1}", self.fps));
                            ui.label(format!("Frame: {:.2}ms", self.frame_time_ms));
                        });
                    }
                });

                ui.add_space(20.0);
                ui.separator();
                ui.add_space(15.0);

                // Color correction settings in a grid
                egui::Grid::new("correction_grid")
                    .num_columns(2)
                    .spacing([20.0, 10.0])
                    .show(ui, |ui| {
                        // Monitor selection (only if multiple monitors)
                        if self.monitors.len() > 1 {
                            ui.label("Monitor:");
                            egui::ComboBox::from_id_salt("monitor_select")
                                .selected_text(format!("{} ({}x{})",
                                    self.monitors[self.selected_monitor].name,
                                    self.monitors[self.selected_monitor].width,
                                    self.monitors[self.selected_monitor].height))
                                .show_ui(ui, |ui| {
                                    let mut monitor_changed = false;

                                    for (idx, monitor) in self.monitors.iter().enumerate() {
                                        let label = format!("{} ({}x{} @ {}Hz){}",
                                            monitor.name,
                                            monitor.width,
                                            monitor.height,
                                            monitor.refresh_rate,
                                            if monitor.is_primary { " [Primary]" } else { "" });

                                        if ui.selectable_value(&mut self.selected_monitor, idx, label).clicked() {
                                            monitor_changed = true;
                                        }
                                    }

                                    // Save and restart outside the borrow
                                    if monitor_changed {
                                        self.config.set_last_monitor(Some(self.selected_monitor));
                                        self.restart_overlay_if_running();
                                    }
                                });
                            ui.end_row();
                        }

                        // Colorblind type selection
                        ui.label("Color Blind Type:");
                        egui::ComboBox::from_id_salt("spectrum_select")
                            .selected_text(self.selected_spectrum
                                .map(|i| self.spectrum_files.get(i).map(|s| s.as_str()).unwrap_or("Invalid"))
                                .unwrap_or("None"))
                            .show_ui(ui, |ui| {
                                let mut selected_spectrum_name: Option<String> = None;

                                for (idx, spectrum) in self.spectrum_files.iter().enumerate() {
                                    if ui.selectable_label(self.selected_spectrum == Some(idx), spectrum).clicked() {
                                        // Validate before selecting
                                        if self.config.validate_spectrum_file(spectrum) {
                                            self.selected_spectrum = Some(idx);
                                            self.status_message = Some(format!("Selected spectrum: {}", spectrum));
                                            selected_spectrum_name = Some(spectrum.clone());
                                        } else {
                                            self.status_message = Some(format!("Spectrum '{}' is invalid", spectrum));
                                            crate::log_info!("User attempted to select invalid spectrum: {}", spectrum);
                                        }
                                    }
                                }

                                // Save and restart outside the borrow
                                if let Some(name) = selected_spectrum_name {
                                    self.config.set_colorblind_type(Some(name));
                                    self.restart_overlay_if_running();
                                }
                            });
                        ui.end_row();

                        // Strength slider (debounced restart)
                        ui.label("Correction Strength:");
                        if ui.add(egui::Slider::new(&mut self.strength, 0.0..=1.0).text("")).changed() {
                            self.config.set_strength(self.strength);
                            self.strength_changed = true;
                            self.strength_last_change = Instant::now();
                        }
                        ui.end_row();

                        // Noise pattern selection
                        ui.label("Noise Pattern:");
                        egui::ComboBox::from_id_salt("noise_select")
                            .selected_text(self.selected_noise
                                .map(|i| self.noise_files.get(i).map(|n| n.as_str()).unwrap_or("Invalid"))
                                .unwrap_or("None"))
                            .show_ui(ui, |ui| {
                                let mut noise_changed = false;
                                let mut selected_noise_name: Option<Option<String>> = None;

                                if ui.selectable_label(self.selected_noise.is_none(), "None").clicked() {
                                    self.selected_noise = None;
                                    self.status_message = Some("Noise texture cleared".to_string());
                                    selected_noise_name = Some(None);
                                    noise_changed = true;
                                }

                                for (idx, noise) in self.noise_files.iter().enumerate() {
                                    if ui.selectable_label(self.selected_noise == Some(idx), noise).clicked() {
                                        // Validate before selecting
                                        if self.config.validate_noise_file(noise) {
                                            self.selected_noise = Some(idx);
                                            self.status_message = Some(format!("Selected noise texture: {}", noise));
                                            selected_noise_name = Some(Some(noise.clone()));
                                            noise_changed = true;
                                        } else {
                                            self.status_message = Some(format!("Noise texture '{}' is invalid", noise));
                                            crate::log_info!("User attempted to select invalid noise texture: {}", noise);
                                        }
                                    }
                                }

                                // Save and restart outside the borrow
                                if noise_changed {
                                    if let Some(name_opt) = selected_noise_name {
                                        self.config.set_noise_texture(name_opt);
                                        self.restart_overlay_if_running();
                                    }
                                }
                            });
                        ui.end_row();
                    });

                ui.add_space(20.0);
                ui.separator();
                ui.add_space(10.0);

                // Advanced settings collapsible section
                egui::CollapsingHeader::new("Advanced Settings")
                    .default_open(self.show_advanced)
                    .show(ui, |ui| {
                        ui.add_space(10.0);

                        // Asset management
                        ui.label("Asset Management:");
                        ui.horizontal(|ui| {
                            if ui.button("Open Asset Folder").clicked() {
                                self.open_asset_folder();
                            }

                            if ui.button("Refresh Assets").clicked() {
                                self.refresh_assets();
                            }
                        });

                        ui.add_space(15.0);

                        // System options
                        ui.label("System Options:");
                        let mut run_at_startup = self.config.get_run_at_startup();
                        if ui.checkbox(&mut run_at_startup, "Run at Windows startup").changed() {
                            self.config.set_run_at_startup(run_at_startup);
                        }

                        ui.add_space(10.0);
                    });

                ui.add_space(15.0);
            });
        });
    }
}

impl eframe::App for ColorInterlacerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for debounced strength restart (250ms after last change)
        if self.strength_changed && self.strength_last_change.elapsed() >= Duration::from_millis(250) {
            self.strength_changed = false;
            self.restart_overlay_if_running();
        }

        // Update FPS every 0.5 seconds
        if self.last_fps_update.elapsed().as_secs_f32() >= 0.5 {
            // TODO: Implement IPC to get actual FPS from overlay
            // For now, just simulate
            if self.overlay_running {
                self.fps = 60.0; // Placeholder
                self.frame_time_ms = 16.7; // Placeholder
            } else {
                self.fps = 0.0;
                self.frame_time_ms = 0.0;
            }
            self.last_fps_update = Instant::now();
        }

        // Draw unified interface
        self.draw_interface(ctx);

        // Request repaint for animations
        ctx.request_repaint();
    }
}

impl Drop for ColorInterlacerApp {
    fn drop(&mut self) {
        // Terminate overlay process when GUI closes
        if let Some(mut child) = self.overlay_process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
