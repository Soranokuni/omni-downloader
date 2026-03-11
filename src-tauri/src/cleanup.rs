use std::time::{SystemTime, Duration};
use std::fs;
use std::path::Path;
use tokio::time;

use crate::config::AppConfig;
use crate::logging;

pub fn spawn_retention_policy(config: AppConfig) {
    tokio::spawn(async move {
        // Run every 24 hours
        let mut interval = time::interval(Duration::from_secs(24 * 3600));
        let nas_dir = config.nas_retention_path.clone();

        // Skip the immediate first tick so startup does not perform filesystem work in dev.
        interval.tick().await;
        
        loop {
            interval.tick().await;
            logging::info("Running 14-day NAS retention policy cleanup...");
            cleanup_old_files(&nas_dir);
        }
    });
}

fn cleanup_old_files(dir: &str) {
    let path = Path::new(dir);
    if !path.exists() {
        return;
    }

    let cutoff_time = SystemTime::now() - Duration::from_secs(14 * 24 * 3600);

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    if let Ok(modified) = metadata.modified() {
                        if modified < cutoff_time {
                            if let Ok(_) = fs::remove_file(entry.path()) {
                                logging::info(format!("Deleted old file: {:?}", entry.path()));
                            }
                        }
                    }
                }
            }
        }
    }
}
