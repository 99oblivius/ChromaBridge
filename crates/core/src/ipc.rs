use serde::{Deserialize, Serialize};

// ===== GUI <-> Overlay IPC =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayConfig {
    pub monitor_index: usize,
    pub spectrum_file: String,
    pub noise_file: Option<String>,
    pub strength: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcCommand {
    Start(OverlayConfig),
    Stop,
    UpdateConfig(OverlayConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayStatus {
    pub fps: f32,
    pub frame_time_ms: f32,
    pub running: bool,
}

// ===== GUI <-> Tray IPC =====

/// Messages sent from GUI to Tray Service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GuiMessage {
    /// GUI is ready and connected
    Ready,
    /// Overlay has started with given spectrum
    OverlayStarted { spectrum: String },
    /// Overlay has stopped
    OverlayStopped,
    /// Periodic status update
    StatusUpdate {
        spectrum: Option<String>,
        overlay_running: bool,
    },
    /// GUI is closing (keep tray running)
    Closing,
    /// Request complete shutdown of tray and overlay
    ExitAll,
}

/// Messages sent from Tray Service to GUI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrayMessage {
    /// Request GUI to show its window
    ShowWindow,
    /// Request to start overlay
    StartOverlay,
    /// Request to stop overlay
    StopOverlay,
    /// Request GUI to exit
    Exit,
}
