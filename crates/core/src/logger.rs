// Unified session-based logging system with automatic rotation
// Used by both GUI and overlay for consistent diagnostics
use anyhow::Result;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::fs;
use std::io::Write;

pub struct SessionLogger {
    log_buffer: Arc<Mutex<Vec<String>>>,
    log_path: PathBuf,
    log_dir: PathBuf,
    retention_count: usize,
    app_name: String,
}

impl SessionLogger {
    pub fn new(log_dir: PathBuf, app_name: &str, retention_count: usize) -> Result<Self> {
        // Create logs directory
        fs::create_dir_all(&log_dir)?;

        // Generate timestamped log filename
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let log_filename = format!("{}_{}.log", app_name, timestamp);
        let log_path = log_dir.join(&log_filename);

        let logger = Self {
            log_buffer: Arc::new(Mutex::new(Vec::new())),
            log_path,
            log_dir,
            retention_count,
            app_name: app_name.to_string(),
        };

        // Clean old logs on startup
        logger.clean_old_logs()?;

        // Write session start
        logger.log(format!("=== {} Session Started ===", app_name));

        Ok(logger)
    }

    pub fn log(&self, message: impl AsRef<str>) {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let log_line = format!("[{}] {}", timestamp, message.as_ref());

        // Buffer in memory for async write on shutdown
        if let Ok(mut buffer) = self.log_buffer.lock() {
            buffer.push(log_line);
        }
    }

    pub fn error(&self, message: impl AsRef<str>) {
        self.log(format!("ERROR: {}", message.as_ref()));
    }

    pub fn warn(&self, message: impl AsRef<str>) {
        self.log(format!("WARN: {}", message.as_ref()));
    }

    pub fn info(&self, message: impl AsRef<str>) {
        self.log(message);
    }

    fn clean_old_logs(&self) -> Result<()> {
        // Get all log files for this app sorted by modification time
        let mut log_files: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
        let prefix = format!("{}_", self.app_name);

        if let Ok(entries) = fs::read_dir(&self.log_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("log") {
                    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                        if filename.starts_with(&prefix) {
                            if let Ok(metadata) = entry.metadata() {
                                if let Ok(modified) = metadata.modified() {
                                    log_files.push((path, modified));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Sort by modification time (newest first)
        log_files.sort_by(|a, b| b.1.cmp(&a.1));

        // Remove old logs beyond retention count
        for (path, _) in log_files.iter().skip(self.retention_count) {
            let _ = fs::remove_file(path);
        }

        Ok(())
    }

    pub fn flush_to_disk(&self) -> Result<()> {
        if let Ok(mut buffer) = self.log_buffer.lock() {
            if buffer.is_empty() {
                return Ok(());
            }

            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.log_path)?;

            for line in buffer.iter() {
                writeln!(file, "{}", line)?;
            }

            file.flush()?;

            // Clear buffer after successful write
            buffer.clear();
        }

        Ok(())
    }

    pub fn finalize(&self) -> Result<()> {
        self.log(format!("=== {} Session Ended ===", self.app_name));
        self.flush_to_disk()?;
        Ok(())
    }
}

impl Drop for SessionLogger {
    fn drop(&mut self) {
        let _ = self.finalize();
    }
}

// Global logger instance
static LOGGER: once_cell::sync::OnceCell<SessionLogger> = once_cell::sync::OnceCell::new();

pub fn init_logger(log_dir: PathBuf, app_name: &str, retention_count: usize) -> Result<()> {
    let logger = SessionLogger::new(log_dir, app_name, retention_count)?;
    LOGGER.set(logger).map_err(|_| anyhow::anyhow!("Logger already initialized"))?;
    Ok(())
}

pub fn log(message: impl AsRef<str>) {
    if let Some(logger) = LOGGER.get() {
        logger.log(message);
    }
}

pub fn log_error(message: impl AsRef<str>) {
    if let Some(logger) = LOGGER.get() {
        logger.error(message);
    }
}

pub fn log_warn(message: impl AsRef<str>) {
    if let Some(logger) = LOGGER.get() {
        logger.warn(message);
    }
}

pub fn log_info(message: impl AsRef<str>) {
    if let Some(logger) = LOGGER.get() {
        logger.info(message);
    }
}

pub fn finalize_logs() -> Result<()> {
    if let Some(logger) = LOGGER.get() {
        logger.finalize()?;
    }
    Ok(())
}

// Convenience macros for formatted logging
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::logger::log_info(format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        $crate::logger::log_warn(format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::logger::log_error(format!($($arg)*))
    };
}
