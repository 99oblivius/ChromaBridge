use crate::StateManager;
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;
use std::path::Path;

#[cfg(windows)]
use windows::{
    core::BOOL,
    Win32::Graphics::Gdi::{
        EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
        DEVMODEW, ENUM_CURRENT_SETTINGS,
    },
    Win32::System::Registry::{
        RegOpenKeyExW, RegSetValueExW, RegDeleteValueW, RegCloseKey,
        HKEY_CURRENT_USER, HKEY, KEY_READ, KEY_WRITE, REG_VALUE_TYPE,
    },
    Win32::Foundation::ERROR_FILE_NOT_FOUND,
};

#[cfg(windows)]
fn check_startup_registry_exists() -> Result<bool> {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::RegQueryValueExW;

    unsafe {
        let subkey = HSTRING::from("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let value_name = HSTRING::from("ChromaBridge");
        let mut hkey = HKEY::default();

        let open_result = RegOpenKeyExW(HKEY_CURRENT_USER, &subkey, None, KEY_READ, &mut hkey);

        if open_result == ERROR_FILE_NOT_FOUND {
            return Ok(false);
        }

        if open_result.is_err() {
            return Err(anyhow::anyhow!("Failed to open registry key: {:?}", open_result));
        }

        let mut buffer = [0u16; 512];
        let mut buffer_size = (buffer.len() * 2) as u32;
        let mut value_type = REG_VALUE_TYPE::default();

        let query_result = RegQueryValueExW(
            hkey,
            &value_name,
            None,
            Some(&mut value_type),
            Some(buffer.as_mut_ptr() as *mut u8),
            Some(&mut buffer_size),
        );

        let _ = RegCloseKey(hkey);
        Ok(query_result.is_ok())
    }
}

#[cfg(windows)]
fn set_startup_registry(enabled: bool, exe_path: &Path) -> Result<()> {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::REG_SZ;

    unsafe {
        let subkey = HSTRING::from("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let value_name = HSTRING::from("ChromaBridge");
        let mut hkey = HKEY::default();

        let open_result = RegOpenKeyExW(HKEY_CURRENT_USER, &subkey, None, KEY_WRITE, &mut hkey);

        if open_result.is_err() {
            return Err(anyhow::anyhow!("Failed to open registry key for write: {:?}", open_result));
        }

        let result = if enabled {
            let path_str = exe_path.to_string_lossy();
            let path_wide: Vec<u16> = path_str.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes: &[u8] = std::slice::from_raw_parts(
                path_wide.as_ptr() as *const u8,
                path_wide.len() * 2
            );

            RegSetValueExW(
                hkey,
                &value_name,
                None,
                REG_SZ,
                Some(bytes),
            )
        } else {
            RegDeleteValueW(hkey, &value_name)
        };

        let _ = RegCloseKey(hkey);

        if result.is_err() {
            return Err(anyhow::anyhow!("Failed to set/delete registry value: {:?}", result));
        }

        Ok(())
    }
}

#[cfg(not(windows))]
fn check_startup_registry_exists() -> Result<bool> {
    Ok(false)
}

#[cfg(not(windows))]
fn set_startup_registry(_enabled: bool, _exe_path: &Path) -> Result<()> {
    Ok(())
}

#[derive(Debug, Clone)]
pub struct MonitorInfo {
    pub index: usize,
    pub name: String,
    pub is_primary: bool,
    pub width: i32,
    pub height: i32,
    pub refresh_rate: u32,
}

#[cfg(windows)]
pub fn enumerate_monitors() -> Result<Vec<MonitorInfo>> {
    use std::sync::Mutex;

    let monitors = Mutex::new(Vec::new());

    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_enum_proc),
            windows::Win32::Foundation::LPARAM(&monitors as *const _ as isize),
        );
    }

    let mut result = monitors.into_inner().unwrap();
    result.sort_by(|a: &MonitorInfo, b: &MonitorInfo| {
        b.is_primary.cmp(&a.is_primary).then(a.index.cmp(&b.index))
    });

    Ok(result)
}

#[cfg(windows)]
unsafe extern "system" fn monitor_enum_proc(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut windows::Win32::Foundation::RECT,
    lparam: windows::Win32::Foundation::LPARAM,
) -> BOOL {
    use std::sync::Mutex;
    let monitors = &*(lparam.0 as *const Mutex<Vec<MonitorInfo>>);

    let mut info: MONITORINFOEXW = std::mem::zeroed();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

    if GetMonitorInfoW(hmonitor, &mut info as *mut _ as *mut _).as_bool() {
        let rect = info.monitorInfo.rcMonitor;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let is_primary = (info.monitorInfo.dwFlags & 1) != 0;

        let name = String::from_utf16_lossy(
            &info.szDevice.iter().take_while(|&&c| c != 0).copied().collect::<Vec<_>>(),
        );

        let refresh_rate = {
            let mut dev_mode: DEVMODEW = std::mem::zeroed();
            dev_mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;

            if EnumDisplaySettingsW(
                windows::core::PCWSTR(info.szDevice.as_ptr()),
                ENUM_CURRENT_SETTINGS,
                &mut dev_mode,
            ).as_bool() {
                dev_mode.dmDisplayFrequency
            } else {
                60
            }
        };

        let mut monitors = monitors.lock().unwrap();
        let index = monitors.len();

        monitors.push(MonitorInfo {
            index,
            name,
            is_primary,
            width,
            height,
            refresh_rate,
        });
    }

    true.into()
}

#[cfg(not(windows))]
pub fn enumerate_monitors() -> Result<Vec<MonitorInfo>> {
    Ok(vec![MonitorInfo {
        index: 0,
        name: "Primary Monitor".to_string(),
        is_primary: true,
        width: 1920,
        height: 1080,
        refresh_rate: 60,
    }])
}

pub struct SettingsGui {
    state: Arc<StateManager>,
    overlay_manager: Arc<crate::overlay::OverlayManager>,

    tray_icon: Option<tray_icon::TrayIcon>,
    overlay_menu_item: Option<tray_icon::menu::CheckMenuItem>,

    monitors: Vec<MonitorInfo>,
    selected_monitor: usize,

    spectrum_files: Vec<String>,
    selected_spectrum: Option<usize>,

    noise_files: Vec<String>,
    selected_noise: Option<usize>,

    strength: f32,
    strength_changed: bool,
    strength_last_change: Instant,

    show_advanced: bool,
    show_developer: bool,
    status_message: Option<String>,

    icon_click_times: Vec<Instant>,

    overlay_toggle_callback: Option<Box<dyn Fn() + Send>>,
    overlay_restart_callback: Option<Box<dyn Fn() + Send>>,

    first_frame: bool,
    close_receiver: Option<crossbeam_channel::Receiver<()>>,
    toggle_receiver: Option<crossbeam_channel::Receiver<()>>,
    app_ctx_storage: Option<Arc<parking_lot::Mutex<Option<egui::Context>>>>,
    dragging: bool,
    icon_texture: Option<egui::TextureHandle>,
}

impl SettingsGui {
    pub fn new(state: Arc<StateManager>, overlay_manager: Arc<crate::overlay::OverlayManager>, ctx_storage: Arc<parking_lot::Mutex<Option<egui::Context>>>) -> Self {
        use crate::log_info;

        log_info!("Initializing SettingsGui");
        let monitors = enumerate_monitors().unwrap_or_default();
        log_info!("Found {} monitors", monitors.len());

        let (selected_monitor, selected_spectrum, selected_noise, strength, show_advanced, show_developer) = state.read(|s| {
            let monitor = s.last_monitor.unwrap_or(0).min(monitors.len().saturating_sub(1));
            let spectrum = s.spectrum_name.as_ref().and_then(|name| {
                state.list_spectrum_files().ok()?.into_iter().position(|s| s == *name)
            });
            let noise = s.noise_texture.as_ref().and_then(|name| {
                state.list_noise_files().ok()?.into_iter().position(|n| n == *name)
            });
            (monitor, spectrum, noise, s.strength, s.show_advanced_settings, false)
        });

        let spectrum_files = state.list_spectrum_files().unwrap_or_default();
        log_info!("Loaded {} spectrum files", spectrum_files.len());

        let noise_files = state.list_noise_files().unwrap_or_default();
        log_info!("Loaded {} noise textures", noise_files.len());

        // Sync startup registry with actual state - registry is source of truth
        let registry_enabled = check_startup_registry_exists().unwrap_or(false);
        state.update(|s| s.run_at_startup = registry_enabled);
        log_info!("Startup registry check: {}", registry_enabled);

        Self {
            state,
            overlay_manager,
            tray_icon: None,
            overlay_menu_item: None,
            monitors,
            selected_monitor,
            spectrum_files,
            selected_spectrum,
            noise_files,
            selected_noise,
            strength,
            strength_changed: false,
            strength_last_change: Instant::now(),
            show_advanced,
            show_developer,
            status_message: None,
            icon_click_times: Vec::new(),
            overlay_toggle_callback: None,
            overlay_restart_callback: None,
            first_frame: true,
            close_receiver: None,
            toggle_receiver: None,
            app_ctx_storage: Some(ctx_storage),
            dragging: false,
            icon_texture: None,
        }
    }

    pub fn set_close_receiver(&mut self, receiver: crossbeam_channel::Receiver<()>) {
        self.close_receiver = Some(receiver);
    }

    pub fn set_toggle_receiver(&mut self, receiver: crossbeam_channel::Receiver<()>) {
        self.toggle_receiver = Some(receiver);
    }

    pub fn set_tray_items(&mut self, tray_icon: tray_icon::TrayIcon, overlay_item: tray_icon::menu::CheckMenuItem) {
        self.tray_icon = Some(tray_icon);
        self.overlay_menu_item = Some(overlay_item);
    }

    pub fn set_overlay_toggle_callback<F>(&mut self, callback: F)
    where
        F: Fn() + Send + 'static,
    {
        self.overlay_toggle_callback = Some(Box::new(callback));
    }

    pub fn set_overlay_restart_callback<F>(&mut self, callback: F)
    where
        F: Fn() + Send + 'static,
    {
        self.overlay_restart_callback = Some(Box::new(callback));
    }

    fn truncate_with_ellipsis(text: &str, max_chars: usize) -> String {
        if text.chars().count() <= max_chars {
            text.to_string()
        } else {
            let mut result: String = text.chars().take(max_chars.saturating_sub(1)).collect();
            result.push('…');
            result
        }
    }

    fn refresh_assets(&mut self) {
        self.spectrum_files = self.state.list_spectrum_files().unwrap_or_default();
        self.noise_files = self.state.list_noise_files().unwrap_or_default();

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

        self.status_message = Some(format!(
            "Refreshed: {} spectrums, {} noise textures",
            self.spectrum_files.len(),
            self.noise_files.len()
        ));
    }

    fn open_asset_folder(&self) {
        #[cfg(windows)]
        {
            use std::process::Command;
            let assets_dir = self.state.app_data_dir().join("assets");
            let _ = Command::new("explorer").arg(assets_dir.to_str().unwrap_or("")).spawn();
        }
    }

    fn restart_overlay_if_needed(&mut self) {
        if let Some(ref callback) = self.overlay_restart_callback {
            callback();
        }
    }

    fn update_tray_state(&self) {
        if let (Some(ref tray_icon), Some(ref overlay_item)) = (&self.tray_icon, &self.overlay_menu_item) {
            let overlay_running = self.overlay_manager.is_running();
            overlay_item.set_checked(overlay_running);

            let spectrum_name = self.state.read(|s| s.spectrum_name.clone());
            let tooltip = if overlay_running {
                if let Some(name) = spectrum_name {
                    format!("ChromaBridge\nOverlay: {} (Active)", name)
                } else {
                    "ChromaBridge\nOverlay: Active".to_string()
                }
            } else {
                "ChromaBridge\nOverlay: Inactive".to_string()
            };
            let _ = tray_icon.set_tooltip(Some(&tooltip));
        }
    }
}

impl eframe::App for SettingsGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        use crate::log_info;

        ctx.style_mut(|style| {
            style.interaction.selectable_labels = false;
        });

        if let Some(ref rx) = self.close_receiver {
            if rx.try_recv().is_ok() {
                log_info!("Close signal received - closing GUI window");
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }

        if let Some(ref rx) = self.toggle_receiver {
            if rx.try_recv().is_ok() {
                log_info!("Toggle signal received from tray menu");
                if let Some(ref callback) = self.overlay_toggle_callback {
                    callback();
                }
                self.update_tray_state();
                ctx.request_repaint();
            }
        }

        if self.first_frame {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);

            if let Some(ref storage) = self.app_ctx_storage {
                *storage.lock() = Some(ctx.clone());
                log_info!("GUI context stored for exit signaling");
            }

            if let Ok(icon_path) = std::env::current_exe() {
                if let Some(parent) = icon_path.parent() {
                    let icon_file = parent.join("icon.ico");
                    if icon_file.exists() {
                        if let Ok(img) = image::open(&icon_file) {
                            let rgba = img.to_rgba8();
                            let size = [rgba.width() as usize, rgba.height() as usize];
                            let pixels = rgba.as_flat_samples();
                            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
                            self.icon_texture = Some(ctx.load_texture("app_icon", color_image, Default::default()));
                        }
                    }
                }
            }

            self.first_frame = false;
        }

        let title_bar_height = 32.0;
        let close_button_size = egui::vec2(46.0, title_bar_height);

        egui::TopBottomPanel::top("title_bar").exact_height(title_bar_height).show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(8.0);

                if let Some(ref texture) = self.icon_texture {
                    let icon_size = 20.0;
                    let icon_response = ui.add(
                        egui::Image::new(texture)
                            .max_size(egui::vec2(icon_size, icon_size))
                            .sense(egui::Sense::click())
                    );

                    if icon_response.clicked() {
                        let now = Instant::now();
                        self.icon_click_times.retain(|&time| now.duration_since(time).as_secs_f32() < 1.0);
                        self.icon_click_times.push(now);

                        if self.icon_click_times.len() >= 5 {
                            log_info!("Developer mode toggled via 5 rapid clicks");
                            self.show_developer = !self.show_developer;
                            self.icon_click_times.clear();
                        }
                    }

                    ui.add_space(8.0);
                }

                let title_response = ui.interact(
                    egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width() - close_button_size.x, title_bar_height)),
                    ui.id().with("title_bar_drag"),
                    egui::Sense::click_and_drag(),
                );

                let primary_down = ctx.input(|i| i.pointer.primary_down());
                if title_response.is_pointer_button_down_on() && primary_down {
                    if !self.dragging {
                        log_info!("Title bar drag started");
                        self.dragging = true;
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                } else if self.dragging {
                    self.dragging = false;
                }

                ui.label(
                    egui::RichText::new("ChromaBridge - Settings")
                        .size(14.0)
                        .strong()
                        .color(egui::Color32::from_rgb(220, 220, 220))
                );

                ui.add_space(6.0);
                let version = env!("CARGO_PKG_VERSION").strip_prefix("0.").unwrap_or(env!("CARGO_PKG_VERSION"));
                ui.label(egui::RichText::new(version).size(10.0).color(egui::Color32::from_rgb(140, 140, 140)));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let close_response = ui.add_sized(
                        close_button_size,
                        egui::Button::new(egui::RichText::new("X").size(16.0))
                            .frame(false)
                    );
                    if close_response.clicked() {
                        log_info!("Close button clicked");
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(10.0);

                let overlay_running = self.overlay_manager.is_running();

                ui.horizontal(|ui| {
                    let button_text = if overlay_running { "Stop Overlay" } else { "Start Overlay" };
                    let button = egui::Button::new(button_text).min_size(egui::vec2(120.0, 30.0));
                    if ui.add(button).clicked() {
                        if let Some(ref callback) = self.overlay_toggle_callback {
                            callback();
                        }
                        self.update_tray_state();
                    }

                    if overlay_running {
                        if let Some((fps, frame_time_ms)) = self.overlay_manager.get_frame_stats() {
                            ui.add_space(10.0);
                            ui.label(format!("{:.1} FPS | {:.2}ms", fps, frame_time_ms));
                        }
                    }
                });

                ui.add_space(20.0);
                ui.separator();
                ui.add_space(15.0);

                egui::Grid::new("correction_grid")
                    .num_columns(2)
                    .spacing([20.0, 10.0])
                    .show(ui, |ui| {
                        if self.monitors.len() > 1 {
                            ui.label("Monitor:");
                            let mut monitor_changed = false;
                            egui::ComboBox::from_id_salt("monitor_select")
                                .selected_text(format!("{} ({}x{})",
                                    self.monitors[self.selected_monitor].name,
                                    self.monitors[self.selected_monitor].width,
                                    self.monitors[self.selected_monitor].height))
                                .show_ui(ui, |ui| {
                                    for (idx, monitor) in self.monitors.iter().enumerate() {
                                        let label = format!("{} ({}x{} @ {}Hz){}",
                                            monitor.name, monitor.width, monitor.height,
                                            monitor.refresh_rate,
                                            if monitor.is_primary { " [Primary]" } else { "" });

                                        if ui.selectable_value(&mut self.selected_monitor, idx, label).clicked() {
                                            monitor_changed = true;
                                        }
                                    }
                                });
                            if monitor_changed {
                                self.state.update(|s| {
                                    s.last_monitor = Some(self.selected_monitor);
                                });
                                self.restart_overlay_if_needed();
                            }
                            ui.end_row();
                        }

                        ui.label("Color Blind Type:");
                        let spectrum_text = self.selected_spectrum
                            .map(|i| self.spectrum_files.get(i).map(|s| Self::truncate_with_ellipsis(s, 30)).unwrap_or_else(|| "Invalid".to_string()))
                            .unwrap_or_else(|| "None".to_string());
                        let mut spectrum_changed = None;
                        egui::ComboBox::from_id_salt("spectrum_select")
                            .selected_text(spectrum_text)
                            .show_ui(ui, |ui| {
                                for (idx, spectrum) in self.spectrum_files.iter().enumerate() {
                                    if ui.selectable_label(self.selected_spectrum == Some(idx), spectrum).clicked() {
                                        self.selected_spectrum = Some(idx);
                                        spectrum_changed = Some(spectrum.clone());
                                    }
                                }
                            });
                        if let Some(spectrum) = spectrum_changed {
                            self.state.update(|s| s.spectrum_name = Some(spectrum));
                            self.restart_overlay_if_needed();
                        }
                        ui.end_row();

                        ui.label("Interlace Pattern:");
                        let noise_text = self.selected_noise
                            .map(|i| self.noise_files.get(i).map(|n| Self::truncate_with_ellipsis(n, 30)).unwrap_or_else(|| "Invalid".to_string()))
                            .unwrap_or_else(|| "None".to_string());
                        let mut noise_changed: Option<Option<String>> = None;
                        egui::ComboBox::from_id_salt("noise_select")
                            .selected_text(noise_text)
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(self.selected_noise.is_none(), "None").clicked() {
                                    self.selected_noise = None;
                                    noise_changed = Some(None);
                                }

                                for (idx, noise) in self.noise_files.iter().enumerate() {
                                    if ui.selectable_label(self.selected_noise == Some(idx), noise).clicked() {
                                        self.selected_noise = Some(idx);
                                        noise_changed = Some(Some(noise.clone()));
                                    }
                                }
                            });
                        if let Some(noise) = noise_changed {
                            self.state.update(|s| s.noise_texture = noise);
                            self.restart_overlay_if_needed();
                        }
                        ui.end_row();

                        ui.label("Correction Strength:");
                        if ui.add(egui::Slider::new(&mut self.strength, 0.0..=1.0).text("")).changed() {
                            self.state.update(|s| s.strength = self.strength);
                            self.strength_changed = true;
                            self.strength_last_change = Instant::now();
                        }
                        ui.end_row();
                    });

                ui.add_space(20.0);
                ui.separator();
                ui.add_space(10.0);

                let header_response = egui::CollapsingHeader::new("Advanced Settings")
                    .default_open(self.show_advanced)
                    .show(ui, |ui| {
                        ui.add_space(10.0);

                        ui.label("Asset Management:");
                        ui.horizontal(|ui| {
                            if ui.button("Open Asset Folder").clicked() {
                                self.open_asset_folder();
                            }

                            if ui.button("↻").clicked() {
                                self.refresh_assets();
                            }
                        });

                        ui.add_space(15.0);

                        ui.label("System Options:");
                        let mut run_at_startup = self.state.read(|s| s.run_at_startup);
                        if ui.checkbox(&mut run_at_startup, "Run at Windows startup").changed() {
                            if let Ok(exe_path) = std::env::current_exe() {
                                if set_startup_registry(run_at_startup, &exe_path).is_ok() {
                                    self.state.update(|s| s.run_at_startup = run_at_startup);
                                }
                            }
                        }

                        let mut open_gui_on_launch = self.state.read(|s| s.open_gui_on_launch);
                        if ui.checkbox(&mut open_gui_on_launch, "Open settings on launch").changed() {
                            self.state.update(|s| s.open_gui_on_launch = open_gui_on_launch);
                        }

                        let mut keep_running_in_tray = self.state.read(|s| s.keep_running_in_tray);
                        if ui.checkbox(&mut keep_running_in_tray, "Keep running in Tray").changed() {
                            self.state.update(|s| s.keep_running_in_tray = keep_running_in_tray);
                        }

                        ui.add_space(10.0);
                    });

                let is_open = header_response.openness > 0.5;
                if is_open != self.show_advanced {
                    self.show_advanced = is_open;
                    self.state.update(|s| s.show_advanced_settings = is_open);
                }

                ui.add_space(15.0);

                // Developer Settings (unlocked by clicking app icon 5 times)
                if self.show_developer {
                    ui.separator();
                    ui.add_space(10.0);

                    let _dev_header_response = egui::CollapsingHeader::new("Developer Settings")
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.add_space(10.0);

                            ui.label("Rendering Options:");
                            let mut cap_to_monitor_refresh = self.state.read(|s| s.cap_to_monitor_refresh);
                            let monitor_hz = if self.selected_monitor < self.monitors.len() {
                                self.monitors[self.selected_monitor].refresh_rate
                            } else {
                                60
                            };

                            if ui.checkbox(&mut cap_to_monitor_refresh, format!("Cap to Monitor Refresh Rate ({}Hz)", monitor_hz)).changed() {
                                self.state.update(|s| s.cap_to_monitor_refresh = cap_to_monitor_refresh);
                                self.restart_overlay_if_needed();
                            }

                            ui.add_space(10.0);
                        });

                    ui.add_space(15.0);
                }

                if let Some(ref msg) = self.status_message {
                    ui.label(msg);
                }
            });
        });

        if self.strength_changed && self.strength_last_change.elapsed() > std::time::Duration::from_millis(500) {
            self.strength_changed = false;
            self.restart_overlay_if_needed();
        }
    }
}

