use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub last_monitor: Option<usize>,
    pub colorblind_type: Option<String>,
    pub strength: f32,
    pub noise_texture: Option<String>,
    pub overlay_enabled: bool,
    pub run_at_startup: bool,
    #[serde(default)]
    pub debug_overlay: bool,
    #[serde(default = "default_log_retention")]
    pub log_retention_count: usize,
}

fn default_log_retention() -> usize {
    10
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            last_monitor: None,
            colorblind_type: None,
            strength: 1.0,
            noise_texture: None,
            overlay_enabled: false,
            run_at_startup: false,
            debug_overlay: false,
            log_retention_count: 10,
        }
    }
}

pub struct Config {
    config_path: PathBuf,
    pub app_data_dir: PathBuf,
    pub spectrums_dir: PathBuf,
    pub noise_dir: PathBuf,
}

impl Config {
    pub fn new() -> Result<Self> {
        let app_data = std::env::var("APPDATA")
            .context("Failed to get APPDATA environment variable")?;

        let app_data_dir = PathBuf::from(app_data).join("ColorInterlacer");
        let config_path = app_data_dir.join("config.json");
        let spectrums_dir = app_data_dir.join("assets").join("spectrums");
        let noise_dir = app_data_dir.join("assets").join("noise");

        // Create directories if they don't exist
        fs::create_dir_all(&app_data_dir)
            .context("Failed to create app data directory")?;
        fs::create_dir_all(&spectrums_dir)
            .context("Failed to create spectrums directory")?;
        fs::create_dir_all(&noise_dir)
            .context("Failed to create noise directory")?;

        Ok(Self {
            config_path,
            app_data_dir,
            spectrums_dir,
            noise_dir,
        })
    }

    pub fn load(&self) -> Result<AppConfig> {
        if !self.config_path.exists() {
            return Ok(AppConfig::default());
        }

        let content = fs::read_to_string(&self.config_path)
            .context("Failed to read config file")?;

        let config: AppConfig = serde_json::from_str(&content)
            .context("Failed to parse config file")?;

        Ok(config)
    }

    pub fn save(&self, config: &AppConfig) -> Result<()> {
        let content = serde_json::to_string_pretty(config)
            .context("Failed to serialize config")?;

        fs::write(&self.config_path, content)
            .context("Failed to write config file")?;

        Ok(())
    }

    /// Validate a spectrum file - returns true if valid, false otherwise
    pub fn validate_spectrum_file(&self, name: &str) -> bool {
        use crate::spectrum::SpectrumPair;

        let path = self.get_spectrum_path(name);

        // Try to load and validate
        match SpectrumPair::load_from_file(path) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    /// Validate a noise texture file - returns true if valid, false otherwise
    pub fn validate_noise_file(&self, name: &str) -> bool {
        use crate::noise::NoiseTexture;

        let path = self.get_noise_path(name);

        // Try to load
        match NoiseTexture::load_from_file(path) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    /// List spectrum files (only includes valid files)
    pub fn list_spectrum_files(&self) -> Result<Vec<String>> {
        let mut files = Vec::new();

        if let Ok(entries) = fs::read_dir(&self.spectrums_dir) {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "json" {
                        if let Some(name) = entry.path().file_stem() {
                            let name_str = name.to_string_lossy().to_string();

                            // Only include if validation passes
                            if self.validate_spectrum_file(&name_str) {
                                files.push(name_str);
                            }
                        }
                    }
                }
            }
        }

        files.sort();
        Ok(files)
    }

    /// List noise files (only includes valid files)
    pub fn list_noise_files(&self) -> Result<Vec<String>> {
        let mut files = Vec::new();

        if let Ok(entries) = fs::read_dir(&self.noise_dir) {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "png" {
                        if let Some(name) = entry.path().file_stem() {
                            let name_str = name.to_string_lossy().to_string();

                            // Only include if validation passes
                            if self.validate_noise_file(&name_str) {
                                files.push(name_str);
                            }
                        }
                    }
                }
            }
        }

        files.sort();
        Ok(files)
    }

    pub fn get_spectrum_path(&self, name: &str) -> PathBuf {
        self.spectrums_dir.join(format!("{}.json", name))
    }

    pub fn get_noise_path(&self, name: &str) -> PathBuf {
        self.noise_dir.join(format!("{}.png", name))
    }
}
