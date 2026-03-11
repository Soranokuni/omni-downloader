use crate::config;
use anyhow::Result;
use std::backtrace::Backtrace;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_FILE_PATH: OnceLock<PathBuf> = OnceLock::new();
static LOG_FILE_LOCK: Mutex<()> = Mutex::new(());

pub fn init_logging() -> Result<PathBuf> {
    let log_dir = config::app_data_dir().join("logs");
    fs::create_dir_all(&log_dir)?;

    let log_file_path = log_dir.join("omni-downloader.log");
    if LOG_FILE_PATH.get().is_none() {
        let _ = LOG_FILE_PATH.set(log_file_path.clone());
    }

    write_line("INFO", "logger", &format!("persistent log initialized at {}", log_file_path.display()));
    Ok(log_file_path)
}

pub fn log_path() -> PathBuf {
    LOG_FILE_PATH
        .get()
        .cloned()
        .unwrap_or_else(|| config::app_data_dir().join("logs").join("omni-downloader.log"))
}

pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let location = panic_info
            .location()
            .map(|value| format!("{}:{}", value.file(), value.line()))
            .unwrap_or_else(|| "unknown location".to_string());

        let payload = panic_info
            .payload()
            .downcast_ref::<&str>()
            .map(|value| (*value).to_string())
            .or_else(|| panic_info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic without string payload".to_string());

        let backtrace = Backtrace::force_capture();
        write_line(
            "ERROR",
            "panic",
            &format!("panic at {}: {}\n{}", location, payload, backtrace),
        );
    }));
}

pub fn info(message: impl AsRef<str>) {
    write_line("INFO", "app", message.as_ref());
}

pub fn error(message: impl AsRef<str>) {
    write_line("ERROR", "app", message.as_ref());
}

fn write_line(level: &str, target: &str, message: &str) {
    let log_file_path = log_path();
    if let Some(parent) = log_file_path.parent() {
        fs::create_dir_all(parent).ok();
    }

    let Ok(_guard) = LOG_FILE_LOCK.lock() else {
        return;
    };

    let timestamp = current_timestamp();
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file_path)
    {
        let _ = writeln!(file, "{} [{}] [{}] {}", timestamp, level, target, message);
    }
}

fn current_timestamp() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => format!("{}.{:03}", duration.as_secs(), duration.subsec_millis()),
        Err(_) => "0.000".to_string(),
    }
}