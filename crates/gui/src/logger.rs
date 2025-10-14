// Session-based logging system with automatic rotation
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
}

impl SessionLogger {
    pub fn new(log_dir: PathBuf, retention_count: usize) -> Result<Self> {
        // Create logs directory
        fs::create_dir_all(&log_dir)?;

        // Generate timestamped log filename
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let log_filename = format!("app_{}.log", timestamp);
        let log_path = log_dir.join(&log_filename);

        let logger = Self {
            log_buffer: Arc::new(Mutex::new(Vec::new())),
            log_path,
            log_dir,
            retention_count,
        };

        // Clean old logs on startup
        logger.clean_old_logs()?;

        // Write session start
        logger.log("=== Color Interlacer GUI Session Started ===");

        Ok(logger)
    }

    pub fn log(&self, message: impl AsRef<str>) {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let log_line = format!("[{}] {}", timestamp, message.as_ref());

        // Print to console (for debug mode)
        println!("{}", log_line);

        // Buffer in memory
        if let Ok(mut buffer) = self.log_buffer.lock() {
            buffer.push(log_line);
        }
    }


    fn clean_old_logs(&self) -> Result<()> {
        // Get all log files sorted by modification time
        let mut log_files: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();

        if let Ok(entries) = fs::read_dir(&self.log_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("log") {
                    if let Ok(metadata) = entry.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            log_files.push((path, modified));
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
        self.log("=== GUI Session Ended ===");
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

pub fn init_logger(log_dir: PathBuf, retention_count: usize) -> Result<()> {
    let logger = SessionLogger::new(log_dir, retention_count)?;
    LOGGER.set(logger).map_err(|_| anyhow::anyhow!("Logger already initialized"))?;
    Ok(())
}

pub fn log(message: impl AsRef<str>) {
    if let Some(logger) = LOGGER.get() {
        logger.log(message);
    }
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::logger::log(format!($($arg)*))
    };
}

pub fn finalize_logs() -> Result<()> {
    if let Some(logger) = LOGGER.get() {
        logger.finalize()?;
    }
    Ok(())
}
