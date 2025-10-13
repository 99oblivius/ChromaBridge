use serde::{Deserialize, Serialize};

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
