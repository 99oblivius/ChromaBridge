// Hide console window on Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ipc;

use anyhow::Result;
use color_interlacer_core::{Config, DbConfig, GuiMessage, TrayMessage, log_info, log_warn, log_error};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tray_icon::{TrayIconBuilder, TrayIconEvent, MouseButton, Icon};
use tray_icon::menu::{Menu, MenuItem, MenuEvent, CheckMenuItem};
use ipc::IpcServer;

/// Application state shared between threads
struct AppState {
    gui_process: Option<Child>,
    overlay_running: bool,
    current_spectrum: Option<String>,
    config: DbConfig,
}

impl AppState {
    fn new() -> Result<Self> {
        let config = DbConfig::new()?;
        Ok(Self {
            gui_process: None,
            overlay_running: false,
            current_spectrum: None,
            config,
        })
    }

    /// Launch GUI process if not already running
    fn ensure_gui_running(&mut self) -> Result<()> {
        // Check if GUI is still alive
        if let Some(ref mut child) = self.gui_process {
            match child.try_wait() {
                Ok(None) => {
                    log_info!("GUI process already running (PID: {})", child.id());
                    return Ok(()); // Still running
                }
                Ok(Some(status)) => {
                    log_info!("GUI process exited with status: {}", status);
                    self.gui_process = None;
                }
                Err(e) => {
                    log_error!("Failed to check GUI process: {}", e);
                    self.gui_process = None;
                }
            }
        }

        // Spawn new GUI process
        let exe_path = std::env::current_exe()?
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Failed to get parent directory"))?
            .join("color-interlacer.exe");

        if !exe_path.exists() {
            return Err(anyhow::anyhow!("GUI executable not found at {:?}", exe_path));
        }

        log_info!("Launching GUI: {:?}", exe_path);
        let child = Command::new(exe_path)
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn GUI: {}", e))?;

        let pid = child.id();
        self.gui_process = Some(child);
        log_info!("GUI process spawned successfully (PID: {})", pid);

        Ok(())
    }

    /// Update state from GUI message
    fn handle_gui_message(&mut self, message: GuiMessage) -> bool {
        match message {
            GuiMessage::Ready => {
                log_info!("GUI is ready");
                false
            }
            GuiMessage::OverlayStarted { spectrum } => {
                log_info!("Overlay started with spectrum: {}", spectrum);
                self.overlay_running = true;
                self.current_spectrum = Some(spectrum);
                self.config.set_overlay_enabled(true);
                false
            }
            GuiMessage::OverlayStopped => {
                log_info!("Overlay stopped");
                self.overlay_running = false;
                self.config.set_overlay_enabled(false);
                false
            }
            GuiMessage::StatusUpdate { spectrum, overlay_running } => {
                log_info!("Status update: overlay={}, spectrum={:?}", overlay_running, spectrum);
                self.overlay_running = overlay_running;
                self.current_spectrum = spectrum;
                false
            }
            GuiMessage::Closing => {
                log_info!("GUI is closing (tray will keep running)");
                self.gui_process = None;
                // Note: overlay_running state is preserved if keep_running_in_tray was enabled
                // The GUI sends a StatusUpdate before closing if overlay should stay running
                false
            }
            GuiMessage::ExitAll => {
                log_info!("GUI requested complete shutdown");
                self.gui_process = None;
                self.overlay_running = false;
                true // Signal to exit
            }
        }
    }

    /// Get current tooltip text
    fn get_tooltip(&self) -> String {
        if self.overlay_running {
            if let Some(ref spectrum) = self.current_spectrum {
                format!("Color Interlacer\nOverlay: {} (Active)", spectrum)
            } else {
                "Color Interlacer\nOverlay: Active".to_string()
            }
        } else {
            "Color Interlacer\nOverlay: Inactive".to_string()
        }
    }

    /// Graceful shutdown
    fn shutdown(&mut self) {
        log_info!("Shutting down...");

        // Kill GUI if running
        if let Some(mut child) = self.gui_process.take() {
            log_info!("Terminating GUI process");
            let _ = child.kill();
            let _ = child.wait();
        }

        // Kill any orphaned overlay processes
        // (These might exist if GUI closed with keep_running_in_tray enabled)
        self.kill_overlay_processes();

        log_info!("Shutdown complete");
    }

    /// Kill all overlay processes by name (for orphaned processes)
    fn kill_overlay_processes(&self) {
        use std::process::Command;

        log_info!("Checking for orphaned overlay processes");

        // Use taskkill to terminate overlay processes
        let result = Command::new("taskkill")
            .args(&["/F", "/IM", "color-interlacer-overlay.exe"])
            .output();

        match result {
            Ok(output) => {
                if output.status.success() {
                    log_info!("Successfully killed overlay processes");
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    // "not found" is not an error - it just means no overlay was running
                    if !stderr.contains("not found") && !stderr.contains("not running") {
                        log_warn!("Overlay kill had issues: {}", stderr);
                    }
                }
            }
            Err(e) => {
                log_error!("Failed to kill overlay processes: {}", e);
            }
        }
    }
}

fn main() -> Result<()> {
    // Initialize logger
    let config = Config::new()?;
    let app_config = config.load().unwrap_or_default();
    let log_dir = config.app_data_dir.join("logs");
    color_interlacer_core::logger::init_logger(log_dir, "tray", app_config.log_retention_count)?;

    log_info!("=== Color Interlacer Tray Service Starting ===");

    // Initialize state
    let state = Arc::new(Mutex::new(AppState::new()?));

    // Start IPC server
    let ipc_server = Arc::new(IpcServer::start()?);

    // Load icon
    let icon = load_icon()?;

    // Create tray menu
    let menu = Menu::new();
    let open_settings_item = MenuItem::new("Open Settings", true, None);
    let overlay_item = CheckMenuItem::new("Enable Overlay", true, false, None);
    let separator = tray_icon::menu::PredefinedMenuItem::separator();
    let exit_item = MenuItem::new("Exit", true, None);

    menu.append(&open_settings_item)?;
    menu.append(&overlay_item)?;
    menu.append(&separator)?;
    menu.append(&exit_item)?;

    // Store menu item IDs
    let open_settings_id = open_settings_item.id().clone();
    let overlay_id = overlay_item.id().clone();
    let exit_id = exit_item.id().clone();

    // Create tray icon
    let tooltip = state.lock().unwrap().get_tooltip();
    let tray_icon = Arc::new(TrayIconBuilder::new()
        .with_menu(Box::new(menu.clone()))
        .with_tooltip(&tooltip)
        .with_icon(icon)
        .build()?);

    log_info!("Tray icon created");

    // Set up tray icon event handler
    let state_clone = Arc::clone(&state);
    TrayIconEvent::set_event_handler(Some(move |event| {
        match event {
            TrayIconEvent::Click { button, .. } => {
                if button == MouseButton::Left {
                    log_info!("Tray icon left-clicked");
                    if let Err(e) = state_clone.lock().unwrap().ensure_gui_running() {
                        log_error!("Failed to launch GUI: {}", e);
                    }
                }
            }
            TrayIconEvent::DoubleClick { button, .. } => {
                if button == MouseButton::Left {
                    log_info!("Tray icon double-clicked");
                    if let Err(e) = state_clone.lock().unwrap().ensure_gui_running() {
                        log_error!("Failed to launch GUI: {}", e);
                    }
                }
            }
            _ => {}
        }
    }));

    // Set up menu event handler
    let state_clone = Arc::clone(&state);
    let ipc_clone = Arc::clone(&ipc_server);
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == open_settings_id {
            log_info!("Open Settings clicked");
            if let Err(e) = state_clone.lock().unwrap().ensure_gui_running() {
                log_error!("Failed to launch GUI: {}", e);
            }
        } else if event.id == overlay_id {
            log_info!("Toggle Overlay clicked");
            let mut state = state_clone.lock().unwrap();
            let new_state = !state.overlay_running;

            // Send command to GUI to toggle overlay
            let message = if new_state {
                TrayMessage::StartOverlay
            } else {
                TrayMessage::StopOverlay
            };

            if let Err(e) = ipc_clone.send(message) {
                log_warn!("Failed to send overlay toggle command: {}. Updating local state anyway.", e);
            }

            state.overlay_running = new_state;
            state.config.set_overlay_enabled(new_state);
            log_info!("Overlay toggled to: {}", new_state);
        } else if event.id == exit_id {
            log_info!("Exit clicked");

            // Send exit command to GUI (which will kill overlay)
            if let Err(e) = ipc_clone.send(TrayMessage::Exit) {
                log_warn!("Failed to send exit command to GUI: {}", e);
            }

            // Give GUI time to shut down gracefully
            std::thread::sleep(std::time::Duration::from_millis(100));

            // Force shutdown
            state_clone.lock().unwrap().shutdown();
            std::process::exit(0);
        }
    }));

    // Always auto-launch GUI on startup
    log_info!("Auto-launching GUI on startup");
    if let Err(e) = state.lock().unwrap().ensure_gui_running() {
        log_error!("Failed to auto-launch GUI: {}", e);
    }

    // Main event loop with Windows message pump
    log_info!("Entering main event loop with message pump");
    let mut last_update = std::time::Instant::now();

    use windows::Win32::UI::WindowsAndMessaging::{PeekMessageW, TranslateMessage, DispatchMessageW, MSG, PM_REMOVE};

    // Windows message pump + periodic updates
    loop {
        unsafe {
            let mut msg = MSG::default();
            // Process all pending messages
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == windows::Win32::UI::WindowsAndMessaging::WM_QUIT {
                    log_info!("WM_QUIT received, exiting");
                    return Ok(());
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        // Process IPC messages from GUI
        while let Some(message) = ipc_server.try_recv() {
            let should_exit = state.lock().unwrap().handle_gui_message(message);
            if should_exit {
                log_info!("Received exit request from GUI, shutting down");
                state.lock().unwrap().shutdown();
                std::process::exit(0);
            }
        }

        // Update tooltip every second
        if last_update.elapsed() >= Duration::from_secs(1) {
            let tooltip = state.lock().unwrap().get_tooltip();
            tray_icon.set_tooltip(Some(&tooltip)).ok();

            // Update overlay checkbox state
            let overlay_running = state.lock().unwrap().overlay_running;
            overlay_item.set_checked(overlay_running);

            last_update = std::time::Instant::now();
        }

        // Check if GUI process has exited
        if let Some(ref mut child) = state.lock().unwrap().gui_process {
            if let Ok(Some(status)) = child.try_wait() {
                log_info!("GUI process exited with status: {}", status);
                state.lock().unwrap().gui_process = None;
            }
        }

        // Sleep briefly to avoid consuming 100% CPU
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Load tray icon from file or create fallback
fn load_icon() -> Result<Icon> {
    let icon_path = std::env::current_exe()?
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Failed to get parent directory"))?
        .join("assets")
        .join("icons")
        .join("icon.ico");

    if icon_path.exists() {
        match Icon::from_path(&icon_path, Some((32, 32))) {
            Ok(icon) => {
                log_info!("Loaded icon from {:?}", icon_path);
                return Ok(icon);
            }
            Err(e) => {
                log_warn!("Failed to load icon from {:?}: {}. Using fallback.", icon_path, e);
            }
        }
    } else {
        log_warn!("Icon file not found at {:?}. Using fallback.", icon_path);
    }

    // Fallback: create simple colored square
    let icon_rgba = {
        let mut rgba = Vec::with_capacity(16 * 16 * 4);
        for _ in 0..16 * 16 {
            rgba.extend_from_slice(&[100, 150, 255, 255]); // Blue color
        }
        rgba
    };

    Icon::from_rgba(icon_rgba, 16, 16)
        .map_err(|e| anyhow::anyhow!("Failed to create fallback icon: {}", e))
}
