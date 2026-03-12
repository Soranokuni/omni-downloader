#[path = "../../src-tauri/src/binaries.rs"]
mod binaries;
#[path = "../../src-tauri/src/cleanup.rs"]
mod cleanup;
#[path = "../../src-tauri/src/config.rs"]
mod config;
#[path = "../../src-tauri/src/core.rs"]
mod core;
#[path = "../../src-tauri/src/logging.rs"]
mod logging;
#[path = "../../src-tauri/src/mcp.rs"]
mod mcp;

use config::AppConfig;
use serde_json::json;
use std::env;

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
    logging::info(format!("headless worker booting, log file: {}", log_path.display()));

    let app_config = match config::load_config() {
        Ok(config) => config,
        Err(error) => {
            logging::error(format!("Failed to load configuration: {}", error));
            std::process::exit(1);
        }
    };
    logging::info(format!(
        "resolved runtime paths: binaries={}, retention={}, output={}",
        app_config.binaries_path, app_config.nas_retention_path, app_config.output_path
    ));

    if let Err(error) = binaries::ensure_binaries(&app_config).await {
        logging::error(format!("Critical error ensuring core binaries: {}", error));
    }

    binaries::spawn_ytdlp_updater(app_config.clone());
    cleanup::spawn_retention_policy(app_config.clone());

    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        std::process::exit(1);
    }

    match args[0].as_str() {
        "--mcp" | "mcp" => {
            logging::info("starting headless worker in MCP mode");
            if let Err(error) = mcp::run_mcp_server(app_config).await {
                logging::error(format!("MCP Server Error: {}", error));
                std::process::exit(1);
            }
        }
        "download" => {
            run_download_command(&args[1..], app_config).await;
        }
        "--help" | "-h" | "help" => {
            print_usage();
        }
        _ => {
            eprintln!("Unknown command: {}", args[0]);
            print_usage();
            std::process::exit(1);
        }
    }
}

async fn run_download_command(args: &[String], config: AppConfig) {
    let url = match argument_value(args, "--url") {
        Some(value) if !value.trim().is_empty() => value,
        _ => {
            eprintln!("Missing required argument: --url");
            print_usage();
            std::process::exit(1);
        }
    };

    let target_filename = argument_value(args, "--target-filename")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "download".to_string());
    let profile_name = argument_value(args, "--profile-name")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| config.default_profile.clone());
    let json_output = args.iter().any(|argument| argument == "--json");

    match core::execute_download_and_ingest(url, target_filename, profile_name, &config, None).await {
        Ok(execution) => {
            if json_output {
                println!(
                    "{}",
                    json!({
                        "ok": true,
                        "exported_files": execution.exported_files,
                        "message": execution.summary()
                    })
                );
            } else {
                println!("{}", execution.summary());
            }
        }
        Err(error) => {
            if json_output {
                println!("{}", json!({ "ok": false, "error": error.to_string() }));
            } else {
                eprintln!("{}", error);
            }
            std::process::exit(1);
        }
    }
}

fn argument_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find_map(|pair| (pair[0] == name).then(|| pair[1].clone()))
}

fn print_usage() {
    eprintln!(
        "Usage:\n  omni-downloader --mcp\n  omni-downloader download --url <media-url> [--target-filename <name>] [--profile-name <profile>] [--json]"
    );
}