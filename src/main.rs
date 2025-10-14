#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gui;
mod overlay;

use anyhow::Result;
use chromabridge::{StateManager, log_info, log_warn};
use std::sync::Arc;
use parking_lot::Mutex;
use std::thread;
use tray_icon::{TrayIconBuilder, TrayIconEvent, MouseButton, Icon};
use tray_icon::menu::{Menu, MenuItem, MenuEvent, CheckMenuItem};

struct App {
    state: Arc<StateManager>,
    overlay_manager: Arc<overlay::OverlayManager>,
    gui_thread: Mutex<Option<thread::JoinHandle<()>>>,
    gui_visible: Arc<Mutex<bool>>,
}

impl App {
    fn new() -> Result<Self> {
        let state = Arc::new(StateManager::new()?);
        let overlay_manager = Arc::new(overlay::OverlayManager::new(Arc::clone(&state)));

        Ok(Self {
            state,
            overlay_manager,
            gui_thread: Mutex::new(None),
            gui_visible: Arc::new(Mutex::new(false)),
        })
    }

    fn open_gui(&self) {
        let mut visible = self.gui_visible.lock();
        if *visible {
            return;
        }
        *visible = true;
        drop(visible);

        log_info!("Opening GUI");

        let state = Arc::clone(&self.state);
        let gui_visible = Arc::clone(&self.gui_visible);
        let overlay_manager = Arc::clone(&self.overlay_manager);

        let handle = thread::spawn(move || {
            let native_options = eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default()
                    .with_inner_size([500.0, 600.0])
                    .with_resizable(false)
                    .with_icon(load_window_icon()),
                ..Default::default()
            };

            let mut settings_gui = gui::SettingsGui::new(state);
            settings_gui.set_overlay_callback(move || {
                overlay_manager.toggle();
            });

            let _ = eframe::run_native(
                "ChromaBridge",
                native_options,
                Box::new(|_cc| Ok(Box::new(settings_gui)))
            );

            *gui_visible.lock() = false;
            log_info!("GUI closed");
        });

        *self.gui_thread.lock() = Some(handle);
    }

    fn toggle_gui(&self) {
        let visible = *self.gui_visible.lock();
        if visible {
            log_info!("GUI already open");
        } else {
            self.open_gui();
        }
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
    let app = Arc::new(App::new()?);

    let log_dir = app.state.app_data_dir().join("logs");
    let log_retention = app.state.read(|s| s.log_retention_count);
    chromabridge::logger::init_logger(log_dir, "chromabridge", log_retention)?;

    log_info!("=== ChromaBridge Starting ===");

    let start_overlay = app.state.read(|s| s.start_overlay_on_launch);
    if start_overlay {
        log_info!("Auto-starting overlay (start_overlay_on_launch=true)");
        app.overlay_manager.start();
    }

    let icon = load_icon()?;

    let menu = Menu::new();
    let open_settings_item = MenuItem::new("Open Settings", true, None);
    let overlay_item = CheckMenuItem::new("Enable Overlay", true, false, None);
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
        .with_tooltip(&tooltip)
        .with_icon(icon)
        .build()?;

    log_info!("Tray icon created");

    let app_clone = Arc::clone(&app);
    TrayIconEvent::set_event_handler(Some(move |event| {
        match event {
            TrayIconEvent::Click { button, .. } | TrayIconEvent::DoubleClick { button, .. } => {
                if button == MouseButton::Left {
                    log_info!("Tray icon clicked");
                    app_clone.toggle_gui();
                }
            }
            _ => {}
        }
    }));

    let app_clone = Arc::clone(&app);
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == open_settings_id {
            log_info!("Open Settings clicked");
            app_clone.toggle_gui();
        } else if event.id == overlay_id {
            log_info!("Toggle Overlay clicked");
            app_clone.toggle_overlay();
        } else if event.id == exit_id {
            log_info!("Exit clicked");
            std::process::exit(0);
        }
    }));

    log_info!("Entering main event loop");

    use windows::Win32::UI::WindowsAndMessaging::{PeekMessageW, TranslateMessage, DispatchMessageW, MSG, PM_REMOVE, WM_QUIT};

    let mut last_update = std::time::Instant::now();

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

        if last_update.elapsed() >= std::time::Duration::from_secs(1) {
            let tooltip = app.get_tooltip();
            tray_icon.set_tooltip(Some(&tooltip)).ok();

            let overlay_running = app.overlay_manager.is_running();
            overlay_item.set_checked(overlay_running);

            last_update = std::time::Instant::now();
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

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
        .map(|p| p.join("assets").join("icons").join("icon.ico"));

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
