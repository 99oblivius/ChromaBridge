use anyhow::{Context, Result};
use parking_lot::RwLock;
use rusqlite::{Connection, params};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use crossbeam_channel::{Sender, Receiver, unbounded};

/// Database schema version for migrations
const SCHEMA_VERSION: i32 = 1;

/// Configuration database with in-memory cache and async writes
pub struct DbConfig {
    app_data_dir: PathBuf,

    // In-memory cache (fast reads)
    cache: Arc<RwLock<ConfigCache>>,

    // Async write channel
    write_sender: Sender<WriteCommand>,
    _write_thread: Option<thread::JoinHandle<()>>,
}

#[derive(Debug, Clone)]
struct ConfigCache {
    // App settings
    last_monitor: Option<usize>,
    colorblind_type: Option<String>,
    strength: f32,
    noise_texture: Option<String>,
    overlay_enabled: bool,
    run_at_startup: bool,
    start_overlay_on_launch: bool,
    keep_running_in_tray: bool,
    advanced_settings_open: bool,
    debug_overlay: bool,
    log_retention_count: usize,
}

impl Default for ConfigCache {
    fn default() -> Self {
        Self {
            last_monitor: None,
            colorblind_type: None,
            strength: 1.0,
            noise_texture: None,
            overlay_enabled: false,
            run_at_startup: false,
            start_overlay_on_launch: false,
            keep_running_in_tray: true, // Default to keep running in tray
            advanced_settings_open: false,
            debug_overlay: false,
            log_retention_count: 10,
        }
    }
}

enum WriteCommand {
    UpdateSetting(String, String),
    Shutdown,
}

impl DbConfig {
    /// Create new database configuration
    pub fn new() -> Result<Self> {
        let app_data = std::env::var("APPDATA")
            .context("Failed to get APPDATA environment variable")?;

        let app_data_dir = PathBuf::from(app_data).join("ColorInterlacer");
        let db_path = app_data_dir.join("config.db");

        // Create directories if they don't exist
        std::fs::create_dir_all(&app_data_dir)
            .context("Failed to create app data directory")?;
        std::fs::create_dir_all(app_data_dir.join("assets").join("spectrums"))
            .context("Failed to create spectrums directory")?;
        std::fs::create_dir_all(app_data_dir.join("assets").join("noise"))
            .context("Failed to create noise directory")?;

        // Initialize database
        let conn = Connection::open(&db_path)
            .context("Failed to open database")?;

        Self::init_database(&conn)?;

        // Load initial cache from database
        let cache = Arc::new(RwLock::new(Self::load_cache(&conn)?));

        // Set up async write thread
        let (write_sender, write_receiver): (Sender<WriteCommand>, Receiver<WriteCommand>) = unbounded();

        let db_path_clone = db_path.clone();
        let write_thread = thread::spawn(move || {
            Self::write_worker(db_path_clone, write_receiver);
        });

        Ok(Self {
            app_data_dir,
            cache,
            write_sender,
            _write_thread: Some(write_thread),
        })
    }

    /// Initialize database schema
    fn init_database(conn: &Connection) -> Result<()> {
        // Enable WAL mode for better concurrency and no journal files
        conn.pragma_update(None, "journal_mode", "WAL")?;

        // Enable synchronous=NORMAL for better performance (still safe with WAL)
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        // Create schema version table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY
            )",
            [],
        )?;

        // Check current version
        let current_version: Option<i32> = conn
            .query_row(
                "SELECT version FROM schema_version LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();

        if current_version.is_none() {
            // First time setup
            Self::create_tables(conn)?;

            conn.execute(
                "INSERT INTO schema_version (version) VALUES (?1)",
                params![SCHEMA_VERSION],
            )?;
        } else if current_version.unwrap() < SCHEMA_VERSION {
            // Run migrations (future use)
            Self::migrate_database(conn, current_version.unwrap())?;
        }

        Ok(())
    }

    /// Create all database tables
    fn create_tables(conn: &Connection) -> Result<()> {
        // Settings table (key-value store)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )?;

        // Initialize default settings
        let defaults = [
            ("last_monitor", "null"),
            ("colorblind_type", "null"),
            ("strength", "1.0"),
            ("noise_texture", "null"),
            ("overlay_enabled", "false"),
            ("run_at_startup", "false"),
            ("debug_overlay", "false"),
            ("log_retention_count", "10"),
        ];

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        for (key, value) in defaults {
            conn.execute(
                "INSERT OR IGNORE INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
                params![key, value, now],
            )?;
        }

        Ok(())
    }

    /// Migrate database to new schema version
    fn migrate_database(_conn: &Connection, _from_version: i32) -> Result<()> {
        // Future migrations go here
        Ok(())
    }

    /// Load cache from database
    fn load_cache(conn: &Connection) -> Result<ConfigCache> {
        let mut cache = ConfigCache::default();

        let get_setting = |key: &str| -> Option<String> {
            conn.query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .ok()
        };

        if let Some(value) = get_setting("last_monitor") {
            cache.last_monitor = if value == "null" {
                None
            } else {
                value.parse().ok()
            };
        }

        cache.colorblind_type = get_setting("colorblind_type")
            .filter(|v| v != "null");

        if let Some(value) = get_setting("strength") {
            cache.strength = value.parse().unwrap_or(1.0);
        }

        cache.noise_texture = get_setting("noise_texture")
            .filter(|v| v != "null");

        if let Some(value) = get_setting("overlay_enabled") {
            cache.overlay_enabled = value == "true";
        }

        if let Some(value) = get_setting("run_at_startup") {
            cache.run_at_startup = value == "true";
        }

        if let Some(value) = get_setting("start_overlay_on_launch") {
            cache.start_overlay_on_launch = value == "true";
        }

        if let Some(value) = get_setting("keep_running_in_tray") {
            cache.keep_running_in_tray = value == "true";
        } else if let Some(value) = get_setting("minimize_to_tray") {
            // Legacy migration from old setting name
            cache.keep_running_in_tray = value == "true";
        }

        if let Some(value) = get_setting("advanced_settings_open") {
            cache.advanced_settings_open = value == "true";
        }

        if let Some(value) = get_setting("debug_overlay") {
            cache.debug_overlay = value == "true";
        }

        if let Some(value) = get_setting("log_retention_count") {
            cache.log_retention_count = value.parse().unwrap_or(10);
        }

        Ok(cache)
    }

    /// Async write worker thread
    fn write_worker(db_path: PathBuf, receiver: Receiver<WriteCommand>) {
        let conn = match Connection::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                crate::log_error!("Failed to open database in write worker: {}", e);
                return;
            }
        };

        // Configure WAL mode in write worker as well
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");

        while let Ok(cmd) = receiver.recv() {
            match cmd {
                WriteCommand::UpdateSetting(key, value) => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64;

                    if let Err(e) = conn.execute(
                        "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
                        params![key, value, now],
                    ) {
                        crate::log_error!("Failed to write setting {}: {}", key, e);
                    }
                }
                WriteCommand::Shutdown => {
                    break;
                }
            }
        }

        // Ensure clean shutdown - checkpoint WAL to main database
        let _ = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE");
    }

    /// Get paths
    pub fn app_data_dir(&self) -> &PathBuf {
        &self.app_data_dir
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.app_data_dir.join("assets")
    }

    pub fn spectrums_dir(&self) -> PathBuf {
        self.app_data_dir.join("assets").join("spectrums")
    }

    pub fn noise_dir(&self) -> PathBuf {
        self.app_data_dir.join("assets").join("noise")
    }

    pub fn get_spectrum_path(&self, name: &str) -> PathBuf {
        self.spectrums_dir().join(format!("{}.json", name))
    }

    pub fn get_noise_path(&self, name: &str) -> PathBuf {
        self.noise_dir().join(format!("{}.png", name))
    }

    // ===== Cache Read Methods (Fast, In-Memory) =====

    pub fn get_last_monitor(&self) -> Option<usize> {
        self.cache.read().last_monitor
    }

    pub fn get_colorblind_type(&self) -> Option<String> {
        self.cache.read().colorblind_type.clone()
    }

    pub fn get_strength(&self) -> f32 {
        self.cache.read().strength
    }

    pub fn get_noise_texture(&self) -> Option<String> {
        self.cache.read().noise_texture.clone()
    }

    pub fn get_overlay_enabled(&self) -> bool {
        self.cache.read().overlay_enabled
    }

    pub fn get_run_at_startup(&self) -> bool {
        self.cache.read().run_at_startup
    }

    pub fn get_start_overlay_on_launch(&self) -> bool {
        self.cache.read().start_overlay_on_launch
    }

    pub fn get_keep_running_in_tray(&self) -> bool {
        self.cache.read().keep_running_in_tray
    }

    pub fn get_advanced_settings_open(&self) -> bool {
        self.cache.read().advanced_settings_open
    }

    pub fn get_debug_overlay(&self) -> bool {
        self.cache.read().debug_overlay
    }

    pub fn get_log_retention_count(&self) -> usize {
        self.cache.read().log_retention_count
    }

    // ===== Cache Write Methods (Updates cache + queues async write) =====

    pub fn set_last_monitor(&self, value: Option<usize>) {
        self.cache.write().last_monitor = value;
        let value_str = value.map(|v| v.to_string()).unwrap_or_else(|| "null".to_string());
        let _ = self.write_sender.send(WriteCommand::UpdateSetting(
            "last_monitor".to_string(),
            value_str,
        ));
    }

    pub fn set_colorblind_type(&self, value: Option<String>) {
        self.cache.write().colorblind_type = value.clone();
        let value_str = value.unwrap_or_else(|| "null".to_string());
        let _ = self.write_sender.send(WriteCommand::UpdateSetting(
            "colorblind_type".to_string(),
            value_str,
        ));
    }

    pub fn set_strength(&self, value: f32) {
        self.cache.write().strength = value;
        let _ = self.write_sender.send(WriteCommand::UpdateSetting(
            "strength".to_string(),
            value.to_string(),
        ));
    }

    pub fn set_noise_texture(&self, value: Option<String>) {
        self.cache.write().noise_texture = value.clone();
        let value_str = value.unwrap_or_else(|| "null".to_string());
        let _ = self.write_sender.send(WriteCommand::UpdateSetting(
            "noise_texture".to_string(),
            value_str,
        ));
    }

    pub fn set_overlay_enabled(&self, value: bool) {
        self.cache.write().overlay_enabled = value;
        let _ = self.write_sender.send(WriteCommand::UpdateSetting(
            "overlay_enabled".to_string(),
            value.to_string(),
        ));
    }

    pub fn set_run_at_startup(&self, value: bool) {
        self.cache.write().run_at_startup = value;
        let _ = self.write_sender.send(WriteCommand::UpdateSetting(
            "run_at_startup".to_string(),
            value.to_string(),
        ));
    }

    pub fn set_start_overlay_on_launch(&self, value: bool) {
        self.cache.write().start_overlay_on_launch = value;
        let _ = self.write_sender.send(WriteCommand::UpdateSetting(
            "start_overlay_on_launch".to_string(),
            value.to_string(),
        ));
    }

    pub fn set_keep_running_in_tray(&self, value: bool) {
        self.cache.write().keep_running_in_tray = value;
        let _ = self.write_sender.send(WriteCommand::UpdateSetting(
            "keep_running_in_tray".to_string(),
            value.to_string(),
        ));
    }

    pub fn set_advanced_settings_open(&self, value: bool) {
        self.cache.write().advanced_settings_open = value;
        let _ = self.write_sender.send(WriteCommand::UpdateSetting(
            "advanced_settings_open".to_string(),
            value.to_string(),
        ));
    }

    pub fn set_debug_overlay(&self, value: bool) {
        self.cache.write().debug_overlay = value;
        let _ = self.write_sender.send(WriteCommand::UpdateSetting(
            "debug_overlay".to_string(),
            value.to_string(),
        ));
    }

    pub fn set_log_retention_count(&self, value: usize) {
        self.cache.write().log_retention_count = value;
        let _ = self.write_sender.send(WriteCommand::UpdateSetting(
            "log_retention_count".to_string(),
            value.to_string(),
        ));
    }

    /// Migrate from old JSON config if it exists
    pub fn migrate_from_json(&self) -> Result<()> {
        let json_path = self.app_data_dir.join("config.json");

        if !json_path.exists() {
            return Ok(()); // Nothing to migrate
        }

        let content = std::fs::read_to_string(&json_path)
            .context("Failed to read old config.json")?;

        let json: serde_json::Value = serde_json::from_str(&content)
            .context("Failed to parse old config.json")?;

        // Migrate each field
        if let Some(value) = json.get("last_monitor").and_then(|v| v.as_u64()) {
            self.set_last_monitor(Some(value as usize));
        }

        if let Some(value) = json.get("colorblind_type").and_then(|v| v.as_str()) {
            self.set_colorblind_type(Some(value.to_string()));
        }

        if let Some(value) = json.get("strength").and_then(|v| v.as_f64()) {
            self.set_strength(value as f32);
        }

        if let Some(value) = json.get("noise_texture").and_then(|v| v.as_str()) {
            self.set_noise_texture(Some(value.to_string()));
        }

        if let Some(value) = json.get("overlay_enabled").and_then(|v| v.as_bool()) {
            self.set_overlay_enabled(value);
        }

        if let Some(value) = json.get("run_at_startup").and_then(|v| v.as_bool()) {
            self.set_run_at_startup(value);
        }

        if let Some(value) = json.get("debug_overlay").and_then(|v| v.as_bool()) {
            self.set_debug_overlay(value);
        }

        if let Some(value) = json.get("log_retention_count").and_then(|v| v.as_u64()) {
            self.set_log_retention_count(value as usize);
        }

        // Rename old config to backup
        let backup_path = self.app_data_dir.join("config.json.bak");
        std::fs::rename(&json_path, &backup_path)
            .context("Failed to backup old config.json")?;

        Ok(())
    }

    /// Validate spectrum file
    pub fn validate_spectrum_file(&self, name: &str) -> bool {
        use crate::spectrum::SpectrumPair;
        let path = self.get_spectrum_path(name);
        SpectrumPair::load_from_file(path).is_ok()
    }

    /// Validate noise file
    pub fn validate_noise_file(&self, name: &str) -> bool {
        use crate::noise::NoiseTexture;
        let path = self.get_noise_path(name);
        NoiseTexture::load_from_file(path).is_ok()
    }

    /// List spectrum files (only valid ones)
    pub fn list_spectrum_files(&self) -> Result<Vec<String>> {
        let mut files = Vec::new();

        if let Ok(entries) = std::fs::read_dir(self.spectrums_dir()) {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "json" {
                        if let Some(name) = entry.path().file_stem() {
                            let name_str = name.to_string_lossy().to_string();
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

    /// List noise files (only valid ones)
    pub fn list_noise_files(&self) -> Result<Vec<String>> {
        let mut files = Vec::new();

        if let Ok(entries) = std::fs::read_dir(self.noise_dir()) {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "png" {
                        if let Some(name) = entry.path().file_stem() {
                            let name_str = name.to_string_lossy().to_string();
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
}

impl Drop for DbConfig {
    fn drop(&mut self) {
        // Signal write thread to shutdown
        let _ = self.write_sender.send(WriteCommand::Shutdown);
    }
}
