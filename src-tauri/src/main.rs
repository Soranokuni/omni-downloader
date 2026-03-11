#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod binaries;
mod cleanup;
mod config;
mod mcp;
mod core;
mod logging;

use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::{Emitter, State};
use config::AppConfig;
use tauri_plugin_log::{Target, TargetKind};

struct AppState {
    config: Arc<Mutex<AppConfig>>,
}

#[tauri::command]
async fn get_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    let cfg = state.config.lock().await;
    Ok(cfg.clone())
}

#[tauri::command]
async fn save_config(new_config: AppConfig, state: State<'_, AppState>) -> Result<(), String> {
    if let Err(e) = config::save_config(&new_config) {
        return Err(e.to_string());
    }
    let mut cfg = state.config.lock().await;
    *cfg = new_config;
    Ok(())
}

#[tauri::command]
async fn start_download(
    url: String, 
    target_filename: String, 
    profile_name: String,
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    let cfg = state.config.lock().await.clone();
    logging::info(format!(
        "queueing download url={} profile={} target={} ",
        url, profile_name, target_filename
    ));
    
    // Fire it in background to not block Tauri IPC
    tauri::async_runtime::spawn(async move {
        if let Err(error) = crate::core::execute_download_and_ingest(url, target_filename, profile_name, &cfg, Some(app_handle.clone())).await {
            logging::error(format!("download task failed: {}", error));
            let _ = app_handle.emit(
                "backend-log",
                crate::core::LogPayload {
                    message: format!("[error] {}", error),
                },
            );
        }
    });

    Ok("Task queued".to_string())
}

#[tokio::main]
async fn main() {
    let log_path = match logging::init_logging() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("Failed to initialize persistent logging: {}", error);
            std::process::exit(1);
        }
    };
    logging::install_panic_hook();
    logging::info(format!("application booting, log file: {}", log_path.display()));

    let app_config = match config::load_config() {
        Ok(c) => c,
        Err(e) => {
            logging::error(format!("Failed to load configuration: {}", e));
            std::process::exit(1);
        }
    };
    logging::info(format!(
        "resolved runtime paths: binaries={}, retention={}, output={}",
        app_config.binaries_path, app_config.nas_retention_path, app_config.output_path
    ));

    if let Err(e) = binaries::ensure_binaries(&app_config).await {
        logging::error(format!("Critical error ensuring core binaries: {}", e));
    }

    binaries::spawn_ytdlp_updater(app_config.clone());
    cleanup::spawn_retention_policy(app_config.clone());

    let args: Vec<String> = env::args().collect();
    if args.iter().any(|arg| arg == "--mcp") {
        logging::info("starting in MCP mode");
        if let Err(e) = mcp::run_mcp_server(app_config).await {
            logging::error(format!("MCP Server Error: {}", e));
        }
    } else {
        logging::info("starting in desktop mode");
        let shared_state = AppState {
            config: Arc::new(Mutex::new(app_config)),
        };

        tauri::Builder::default()
            .manage(shared_state)
            .plugin(
                tauri_plugin_log::Builder::new()
                    .clear_targets()
                    .targets([
                        Target::new(TargetKind::Stdout),
                        Target::new(TargetKind::Webview),
                    ])
                    .level(log::LevelFilter::Info)
                    .build(),
            )
            .invoke_handler(tauri::generate_handler![get_config, save_config, start_download])
            .run(tauri::generate_context!())
            .expect("error while running tauri application");
    }
}
