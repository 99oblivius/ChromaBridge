use anyhow::{Context, Result};
use parking_lot::RwLock;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use crossbeam_channel::{Sender, Receiver, unbounded};

const SCHEMA_VERSION: i32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub last_monitor: Option<usize>,
    pub spectrum_name: Option<String>,
    pub strength: f32,
    pub noise_texture: Option<String>,
    pub overlay_enabled: bool,
    pub run_at_startup: bool,
    pub start_overlay_on_launch: bool,
    pub keep_running_in_tray: bool,
    pub debug_overlay: bool,
    pub log_retention_count: usize,

    #[serde(default = "default_open_gui_on_launch")]
    pub open_gui_on_launch: bool,
    #[serde(default)]
    pub show_advanced_settings: bool,
    #[serde(default)]
    pub last_overlay_enabled: bool,

    #[serde(default = "default_vsync_enabled")]
    pub vsync_enabled: bool,
    #[serde(default = "default_target_fps")]
    pub target_fps: Option<f32>,
}

fn default_vsync_enabled() -> bool {
    true
}

fn default_target_fps() -> Option<f32> {
    None // Default to no FPS cap
}

fn default_open_gui_on_launch() -> bool {
    true
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            last_monitor: None,
            spectrum_name: None,
            strength: 1.0,
            noise_texture: None,
            overlay_enabled: false,
            run_at_startup: false,
            start_overlay_on_launch: false,
            keep_running_in_tray: true,
            debug_overlay: false,
            log_retention_count: 10,

            open_gui_on_launch: true,
            show_advanced_settings: false,
            last_overlay_enabled: false,

            vsync_enabled: true,
            target_fps: None,
        }
    }
}

enum WriteCommand {
    Update(AppState),
    Shutdown,
}

pub struct StateManager {
    app_data_dir: PathBuf,
    state: Arc<RwLock<AppState>>,
    write_sender: Sender<WriteCommand>,
    _write_thread: Option<thread::JoinHandle<()>>,
}

impl StateManager {
    pub fn new() -> Result<Self> {
        let app_data = std::env::var("APPDATA")
            .context("Failed to get APPDATA environment variable")?;

        let app_data_dir = PathBuf::from(app_data).join("ChromaBridge");
        let db_path = app_data_dir.join("state.db");

        std::fs::create_dir_all(&app_data_dir)
            .context("Failed to create app data directory")?;
        std::fs::create_dir_all(app_data_dir.join("assets").join("spectrums"))
            .context("Failed to create spectrums directory")?;
        std::fs::create_dir_all(app_data_dir.join("assets").join("noise"))
            .context("Failed to create noise directory")?;

        let conn = Connection::open(&db_path).context("Failed to open database")?;
        Self::init_database(&conn)?;

        let initial_state = Self::load_state(&conn)?;
        let state = Arc::new(RwLock::new(initial_state));

        let (write_sender, write_receiver): (Sender<WriteCommand>, Receiver<WriteCommand>) = unbounded();

        let db_path_clone = db_path.clone();
        let write_thread = thread::spawn(move || {
            Self::write_worker(db_path_clone, write_receiver);
        });

        Ok(Self {
            app_data_dir,
            state,
            write_sender,
            _write_thread: Some(write_thread),
        })
    }

    fn init_database(conn: &Connection) -> Result<()> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY)",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;

        let current_version: Option<i32> = conn
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| row.get(0))
            .ok();

        if current_version.is_none() {
            conn.execute("INSERT INTO schema_version (version) VALUES (?1)", params![SCHEMA_VERSION])?;
        }

        Ok(())
    }

    fn load_state(conn: &Connection) -> Result<AppState> {
        let json_str: Option<String> = conn
            .query_row("SELECT value FROM state WHERE key = 'app_state'", [], |row| row.get(0))
            .ok();

        match json_str {
            Some(json) => {
                serde_json::from_str(&json).context("Failed to parse state JSON")
            }
            None => Ok(AppState::default()),
        }
    }

    fn write_worker(db_path: PathBuf, receiver: Receiver<WriteCommand>) {
        let conn = match Connection::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                crate::log_error!("Failed to open database in write worker: {}", e);
                return;
            }
        };

        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");

        while let Ok(cmd) = receiver.recv() {
            match cmd {
                WriteCommand::Update(state) => {
                    if let Ok(json) = serde_json::to_string(&state) {
                        if let Err(e) = conn.execute(
                            "INSERT OR REPLACE INTO state (key, value) VALUES ('app_state', ?1)",
                            params![json],
                        ) {
                            crate::log_error!("Failed to write state: {}", e);
                        }
                    }
                }
                WriteCommand::Shutdown => {
                    break;
                }
            }
        }

        let _ = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE");
    }

    pub fn app_data_dir(&self) -> &PathBuf {
        &self.app_data_dir
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

    pub fn read<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&AppState) -> R,
    {
        f(&self.state.read())
    }

    pub fn update<F>(&self, f: F)
    where
        F: FnOnce(&mut AppState),
    {
        let mut state = self.state.write();
        f(&mut state);
        let _ = self.write_sender.send(WriteCommand::Update(state.clone()));
    }

    pub fn list_spectrum_files(&self) -> Result<Vec<String>> {
        use crate::SpectrumPair;
        let mut files = Vec::new();

        if let Ok(entries) = std::fs::read_dir(self.spectrums_dir()) {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "json" {
                        if let Some(name) = entry.path().file_stem() {
                            let name_str = name.to_string_lossy().to_string();
                            let path = self.get_spectrum_path(&name_str);
                            if SpectrumPair::load_from_file(path).is_ok() {
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

    pub fn list_noise_files(&self) -> Result<Vec<String>> {
        use crate::NoiseTexture;
        let mut files = Vec::new();

        if let Ok(entries) = std::fs::read_dir(self.noise_dir()) {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "png" {
                        if let Some(name) = entry.path().file_stem() {
                            let name_str = name.to_string_lossy().to_string();
                            let path = self.get_noise_path(&name_str);
                            if NoiseTexture::load_from_file(path).is_ok() {
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

impl Drop for StateManager {
    fn drop(&mut self) {
        let _ = self.write_sender.send(WriteCommand::Shutdown);
    }
}
