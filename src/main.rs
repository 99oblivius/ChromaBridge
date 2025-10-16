#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gui;
mod overlay;

use anyhow::Result;
use chromabridge::{StateManager, log_info, log_warn};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use crossbeam_channel::{Sender, Receiver, bounded};
use tray_icon::{TrayIconBuilder, TrayIconEvent, MouseButton, Icon};
use tray_icon::menu::{Menu, MenuItem, MenuEvent, CheckMenuItem};

#[derive(Debug)]
enum AppCommand {
    OpenGui,
    ToggleOverlay,
    Exit,
}

struct App {
    state: Arc<StateManager>,
    overlay_manager: Arc<overlay::OverlayManager>,
    gui_visible: Arc<AtomicBool>,
    exit_requested: Arc<AtomicBool>,
    command_tx: Sender<AppCommand>,
    gui_close_tx: parking_lot::Mutex<Option<Sender<()>>>,
    gui_toggle_tx: parking_lot::Mutex<Option<Sender<()>>>,
    gui_ctx: Arc<parking_lot::Mutex<Option<egui::Context>>>,
    wakeup: Arc<(parking_lot::Mutex<()>, parking_lot::Condvar)>,
}

impl App {
    fn new() -> Result<(Self, Receiver<AppCommand>)> {
        let state = Arc::new(StateManager::new()?);
        let overlay_manager = Arc::new(overlay::OverlayManager::new(Arc::clone(&state)));
        let (command_tx, command_rx) = bounded(10);

        Ok((Self {
            state,
            overlay_manager,
            gui_visible: Arc::new(AtomicBool::new(false)),
            exit_requested: Arc::new(AtomicBool::new(false)),
            command_tx,
            gui_close_tx: parking_lot::Mutex::new(None),
            gui_toggle_tx: parking_lot::Mutex::new(None),
            gui_ctx: Arc::new(parking_lot::Mutex::new(None)),
            wakeup: Arc::new((parking_lot::Mutex::new(()), parking_lot::Condvar::new())),
        }, command_rx))
    }

    fn request_open_gui(&self) {
        if !self.gui_visible.load(Ordering::Acquire) {
            let _ = self.command_tx.try_send(AppCommand::OpenGui);
            self.wakeup.1.notify_one();
        } else {
            log_info!("GUI already open - bringing to front");
            if let Some(ctx) = self.gui_ctx.lock().as_ref() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                ctx.request_repaint();
            }
        }
    }

    fn request_toggle_overlay(&self) {
        if self.gui_visible.load(Ordering::Acquire) {
            if let Some(toggle_tx) = self.gui_toggle_tx.lock().as_ref() {
                let _ = toggle_tx.try_send(());
            }

            if let Some(ctx) = self.gui_ctx.lock().as_ref() {
                ctx.request_repaint();
            }

        } else {
            let _ = self.command_tx.try_send(AppCommand::ToggleOverlay);
            self.wakeup.1.notify_one();
        }
    }

    fn request_exit(&self) {
        self.exit_requested.store(true, Ordering::Release);

        // Force GUI to repaint so it checks the close signal
        if let Some(ctx) = self.gui_ctx.lock().as_ref() {
            ctx.request_repaint();
        }

        if let Some(close_tx) = self.gui_close_tx.lock().as_ref() {
            let _ = close_tx.try_send(());
        }

        let _ = self.command_tx.try_send(AppCommand::Exit);
        self.wakeup.1.notify_one();
    }

    fn toggle_overlay(&self) {
        self.overlay_manager.toggle();
    }

    fn get_tooltip(&self) -> String {
        let overlay_running = self.overlay_manager.is_running();
        let spectrum_name = self.state.read(|s| s.spectrum_name.clone());

        if overlay_running {
            if let Some(name) = spectrum_name {
                format!("ChromaBridge\nOverlay: {} (Active)", name)
            } else {
                "ChromaBridge\nOverlay: Active".to_string()
            }
        } else {
            "ChromaBridge\nOverlay: Inactive".to_string()
        }
    }
}

fn main() -> Result<()> {
    let result = run_app();
    let _ = chromabridge::logger::finalize_logs();
    result
}

fn run_app() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let enable_file_logging = args.contains(&"--stream-logs".to_string());

    let (app, command_rx) = App::new()?;
    let app = Arc::new(app);

    let log_dir = app.state.app_data_dir().join("logs");
    let log_retention = app.state.read(|s| s.log_retention_count);
    chromabridge::logger::init_logger(log_dir, "chromabridge", log_retention, enable_file_logging)?;

    log_info!("ChromaBridge main() started");
    if let Some(log_path) = chromabridge::logger::get_log_path() {
        log_info!("Log file: {}", log_path.display());
    }
    if enable_file_logging {
        log_info!("Streaming mode enabled via --stream-logs");
    } else {
        log_info!("Buffered mode - logs will be written to file on exit");
    }

    log_info!("=== ChromaBridge Starting ===");

    let last_overlay_enabled = app.state.read(|s| s.last_overlay_enabled);
    if last_overlay_enabled {
        log_info!("Restoring overlay (was enabled on last shutdown)");
        app.overlay_manager.start();
    }

    let open_gui = app.state.read(|s| s.open_gui_on_launch);
    if open_gui {
        log_info!("Auto-opening GUI (open_gui_on_launch=true)");
        app.request_open_gui();
    }

    log_info!("Loading tray icon");
    let icon = load_icon()?;

    let initial_overlay_state = app.overlay_manager.is_running();

    let menu = Menu::new();
    let open_settings_item = MenuItem::new("Open Settings", true, None);
    let overlay_item = CheckMenuItem::new("Enable Overlay", true, initial_overlay_state, None);
    let separator = tray_icon::menu::PredefinedMenuItem::separator();
    let exit_item = MenuItem::new("Exit", true, None);

    menu.append(&open_settings_item)?;
    menu.append(&overlay_item)?;
    menu.append(&separator)?;
    menu.append(&exit_item)?;

    let open_settings_id = open_settings_item.id().clone();
    let overlay_id = overlay_item.id().clone();
    let exit_id = exit_item.id().clone();

    let tooltip = app.get_tooltip();
    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu.clone()))
        .with_menu_on_left_click(false)
        .with_tooltip(&tooltip)
        .with_icon(icon.clone())
        .build()?;

    log_info!("Tray icon created on main thread");

    let app_clone = Arc::clone(&app);
    let exit_requested_for_click = Arc::clone(&app.exit_requested);
    TrayIconEvent::set_event_handler(Some(move |event| {
        match event {
            TrayIconEvent::Click { button, .. } | TrayIconEvent::DoubleClick { button, .. } => {
                if button == MouseButton::Left {
                    if exit_requested_for_click.load(Ordering::Acquire) {
                        return;
                    }
                    log_info!("Tray icon clicked");
                    app_clone.request_open_gui();
                }
            }
            _ => {}
        }
    }));

    let app_clone = Arc::clone(&app);
    let exit_requested_for_menu = Arc::clone(&app.exit_requested);
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == open_settings_id {
            if exit_requested_for_menu.load(Ordering::Acquire) {
                return;
            }
            log_info!("Open Settings clicked");
            app_clone.request_open_gui();
        } else if event.id == overlay_id {
            let was_running = app_clone.overlay_manager.is_running();
            let state = if was_running { "OFF" } else { "ON" };
            log_info!("Toggle Overlay clicked (turning {})", state);
            app_clone.request_toggle_overlay();
        } else if event.id == exit_id {
            log_info!("Exit clicked");
            app_clone.request_exit();
        }
    }));

    log_info!("Entering main event loop");

    use windows::Win32::UI::WindowsAndMessaging::{PeekMessageW, TranslateMessage, DispatchMessageW, MSG, PM_REMOVE, WM_QUIT};

    let mut last_tray_update = std::time::Instant::now();
    let wakeup = Arc::clone(&app.wakeup);

    loop {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    log_info!("WM_QUIT received, exiting");
                    return Ok(());
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        // Check if we should update tray (either timer elapsed or explicitly requested)
        let should_update_tray = last_tray_update.elapsed() >= std::time::Duration::from_millis(100);

        if should_update_tray {
            let tooltip = app.get_tooltip();
            tray_icon.set_tooltip(Some(&tooltip)).ok();

            let overlay_running = app.overlay_manager.is_running();
            overlay_item.set_checked(overlay_running);

            last_tray_update = std::time::Instant::now();
        }

        let mut processed_toggle = false;
        while let Ok(cmd) = command_rx.try_recv() {
            match cmd {
            AppCommand::OpenGui => {
                if app.exit_requested.load(Ordering::Acquire) {
                    log_info!("Exit requested, ignoring GUI open request");
                    continue;
                }

                if app.gui_visible.swap(true, Ordering::AcqRel) {
                    log_info!("GUI already open, ignoring duplicate open request");
                    continue;
                }

                log_info!("Opening GUI window");

                let state = Arc::clone(&app.state);
                let overlay_manager = Arc::clone(&app.overlay_manager);
                let gui_visible = Arc::clone(&app.gui_visible);
                let command_tx_for_requeue = app.command_tx.clone();
                let gui_ctx_storage = Arc::clone(&app.gui_ctx);
                let wakeup = Arc::clone(&app.wakeup);

                let (close_tx, close_rx) = bounded(1);
                *app.gui_close_tx.lock() = Some(close_tx);

                let (toggle_tx, toggle_rx) = bounded(1);
                *app.gui_toggle_tx.lock() = Some(toggle_tx);

                let native_options = eframe::NativeOptions {
                    viewport: egui::ViewportBuilder::default()
                        .with_inner_size([500.0, 600.0])
                        .with_resizable(false)
                        .with_decorations(false)
                        .with_icon(load_window_icon()),
                    run_and_return: true,
                    ..Default::default()
                };

                let overlay_manager_for_gui = Arc::clone(&overlay_manager);
                let overlay_manager_for_toggle = Arc::clone(&overlay_manager);
                let overlay_manager_for_restart = Arc::clone(&overlay_manager);
                let wakeup_for_toggle = Arc::clone(&wakeup);
                let gui_ctx_storage_for_gui = Arc::clone(&gui_ctx_storage);
                let state_for_gui = Arc::clone(&state);
                let tray_icon_for_gui = tray_icon.clone();
                let overlay_item_for_gui = overlay_item.clone();

                let result = eframe::run_native(
                    "ChromaBridge",
                    native_options,
                    Box::new(move |_cc| {
                        let mut settings_gui = gui::SettingsGui::new(state_for_gui, overlay_manager_for_gui, gui_ctx_storage_for_gui);
                        settings_gui.set_tray_items(tray_icon_for_gui, overlay_item_for_gui);
                        settings_gui.set_close_receiver(close_rx);
                        settings_gui.set_toggle_receiver(toggle_rx);
                        settings_gui.set_overlay_toggle_callback(move || {
                            let was_running = overlay_manager_for_toggle.is_running();
                            overlay_manager_for_toggle.toggle();
                            let state = if was_running { "OFF" } else { "ON" };
                            log_info!("Overlay toggled from GUI: {}", state);
                            wakeup_for_toggle.1.notify_one();
                        });

                        settings_gui.set_overlay_restart_callback(move || {
                            if overlay_manager_for_restart.is_running() {
                                log_info!("Restarting overlay (settings changed)");
                                overlay_manager_for_restart.stop();
                                overlay_manager_for_restart.start();
                            }
                        });

                        Ok(Box::new(settings_gui))
                    })
                );

                if let Err(e) = result {
                    log_warn!("GUI window error: {:?}", e);
                }
                *app.gui_close_tx.lock() = None;
                *app.gui_toggle_tx.lock() = None;
                *gui_ctx_storage.lock() = None;
                gui_visible.store(false, Ordering::Release);
                log_info!("GUI window closed");

                // Drain any buffered OpenGui commands to prevent immediate reopening
                let mut drained = 0;
                let mut other_commands = Vec::new();
                while let Ok(cmd) = command_rx.try_recv() {
                    match cmd {
                        AppCommand::OpenGui => {
                            drained += 1;
                        }
                        other => {
                            other_commands.push(other);
                        }
                    }
                }
                for cmd in other_commands {
                    let _ = command_tx_for_requeue.try_send(cmd);
                }
                if drained > 0 {
                    log_info!("Drained {} buffered OpenGui commands", drained);
                }

                let keep_in_tray = state.read(|s| s.keep_running_in_tray);
                if !keep_in_tray {
                    log_info!("Keep in tray disabled - exiting application");
                    return Ok(());
                }
            }
            AppCommand::ToggleOverlay => {
                app.toggle_overlay();
                processed_toggle = true;
            }
            AppCommand::Exit => {
                log_info!("Exit command - shutting down application");
                app.exit_requested.store(true, Ordering::Release);
                return Ok(());
            }
            }
        }

        // Update tray immediately if we processed a toggle command
        if processed_toggle {
            let tooltip = app.get_tooltip();
            tray_icon.set_tooltip(Some(&tooltip)).ok();
            let overlay_running = app.overlay_manager.is_running();
            overlay_item.set_checked(overlay_running);
            last_tray_update = std::time::Instant::now();
        }

        let mut guard = wakeup.0.lock();
        let result = wakeup.1.wait_for(&mut guard, std::time::Duration::from_millis(100));

        // If woken by notification (not timeout), update tray immediately
        // This handles GUI button toggle which wakes us via condvar
        if !result.timed_out() && !processed_toggle {
            let tooltip = app.get_tooltip();
            tray_icon.set_tooltip(Some(&tooltip)).ok();
            let overlay_running = app.overlay_manager.is_running();
            overlay_item.set_checked(overlay_running);
            last_tray_update = std::time::Instant::now();
        }
    }

    #[allow(unreachable_code)]
    Ok(())
}

fn load_icon() -> Result<Icon> {
    let icon_path = std::env::current_exe()?
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Failed to get parent directory"))?
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

    let icon_rgba = {
        let mut rgba = Vec::with_capacity(16 * 16 * 4);
        for _ in 0..16 * 16 {
            rgba.extend_from_slice(&[100, 150, 255, 255]);
        }
        rgba
    };

    Icon::from_rgba(icon_rgba, 16, 16)
        .map_err(|e| anyhow::anyhow!("Failed to create fallback icon: {}", e))
}

fn load_window_icon() -> egui::IconData {
    let icon_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .map(|p| p.join("icon.ico"));

    if let Some(path) = icon_path {
        if path.exists() {
            if let Ok(image) = image::open(&path) {
                let rgba = image.to_rgba8();
                let (width, height) = rgba.dimensions();
                return egui::IconData {
                    rgba: rgba.into_raw(),
                    width,
                    height,
                };
            }
        }
    }

    let mut rgba = Vec::with_capacity(32 * 32 * 4);
    for _ in 0..32 * 32 {
        rgba.extend_from_slice(&[100, 150, 255, 255]);
    }

    egui::IconData {
        rgba,
        width: 32,
        height: 32,
    }
}
