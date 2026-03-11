use anyhow::Result;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

use crate::config::AppConfig;
use crate::logging;

pub async fn run_mcp_server(config: AppConfig) -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut handle = stdin.lock();

    let mut line = String::new();
    while handle.read_line(&mut line)? > 0 {
        if line.trim().is_empty() {
            line.clear();
            continue;
        }

        match serde_json::from_str::<Value>(&line) {
            Ok(req) => {
                let id = req.get("id");
                let method = req.get("method").and_then(Value::as_str).unwrap_or("");

                let response = match method {
                    "initialize" => {
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "protocolVersion": "2024-11-05",
                                "capabilities": {
                                    "tools": { "listChanged": false }
                                },
                                "serverInfo": {
                                    "name": "omni-downloader-mcp",
                                    "version": "1.0.0"
                                }
                            }
                        })
                    }
                    "tools/list" => {
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "tools": [
                                    {
                                        "name": "extract_article_media",
                                        "description": "Extracts iframe media URL and summary from an article URL.",
                                        "inputSchema": {
                                            "type": "object",
                                            "properties": {
                                                "url": { "type": "string" }
                                            },
                                            "required": ["url"]
                                        }
                                    },
                                    {
                                        "name": "download_and_ingest_dalet",
                                        "description": "Downloads media from a direct link or article URL, then transcodes it using the selected profile.",
                                        "inputSchema": {
                                            "type": "object",
                                            "properties": {
                                                "url": { "type": "string" },
                                                "target_filename": { "type": "string" },
                                                "profile_name": { "type": "string" },
                                                "email": { "type": "string" },
                                                "order": { "type": "integer" }
                                            },
                                            "required": ["url"]
                                        }
                                    }
                                ]
                            }
                        })
                    }
                    "tools/call" => {
                        let empty_params = json!({});
                        let params = req.get("params").unwrap_or(&empty_params);
                        let tool_name = params.get("name").and_then(Value::as_str).unwrap_or("");
                        let tool_args = params.get("arguments").cloned().unwrap_or(json!({}));

                        let result = match tool_name {
                            "extract_article_media" => {
                                handle_extract_article_media(tool_args).await
                            }
                            "download_and_ingest_dalet" => {
                                handle_download_and_ingest_dalet(tool_args, &config).await
                            }
                            _ => json!({ "isError": true, "content": [{"type": "text", "text": "Tool not found"}] }),
                        };

                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": result
                        })
                    }
                    _ => {
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32601,
                                "message": "Method not found"
                            }
                        })
                    }
                };

                // Stdout
                let mut out = stdout.lock();
                let _ = writeln!(out, "{}", response.to_string());
                let _ = out.flush();
            }
            Err(e) => {
                logging::error(format!("Failed to parse JSON-RPC: {}", e));
            }
        }
        line.clear();
    }
    Ok(())
}

async fn handle_extract_article_media(args: Value) -> Value {
    let url = match args.get("url").and_then(Value::as_str) {
        Some(u) => u,
        None => return json!({ "isError": true, "content": [{"type": "text", "text": "Missing 'url' argument"}] }),
    };

    match fetch_and_parse_article(url).await {
        Ok((iframe_src, title)) => {
            json!({
                "content": [
                    { "type": "text", "text": format!("Found iframe: {}\nSummary Title: {}", iframe_src, title) }
                ]
            })
        }
        Err(e) => {
            logging::error(format!("Error extracting media: {}", e));
            json!({ "isError": true, "content": [{"type": "text", "text": format!("Error extracting media: {}", e)}] })
        }
    }
}

async fn fetch_and_parse_article(url: &str) -> Result<(String, String)> {
    let html = reqwest::get(url).await?.text().await?;
    let document = scraper::Html::parse_document(&html);
    
    let h1_selector = scraper::Selector::parse("h1").unwrap();
    let iframe_selector = scraper::Selector::parse("iframe").unwrap();
    
    let title = document.select(&h1_selector).next()
        .map(|node| node.text().collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|| "Unknown Title".to_string());
        
    let mut best_iframe = String::new();
    for node in document.select(&iframe_selector) {
        if let Some(src) = node.value().attr("src") {
            // Assume the first iframe is the primary video for this basic implementation.
            // In a real scenario, we might match the src or class against the title.
            best_iframe = src.to_string();
            break;
        }
    }
    
    if best_iframe.is_empty() {
        anyhow::bail!("No iframe found in the article.");
    }
    
    Ok((best_iframe, title))
}

async fn handle_download_and_ingest_dalet(args: Value, config: &AppConfig) -> Value {
    let url = match args.get("url").and_then(Value::as_str) {
        Some(u) => u,
        None => return json!({ "isError": true, "content": [{"type": "text", "text": "Missing 'url' argument"}] }),
    };

    let target_filename = args
        .get("target_filename")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| build_mcp_target_name(&args));

    let profile_name = match args.get("profile_name").and_then(Value::as_str) {
        Some(u) => u.to_string(),
        None => config.default_profile.clone(),
    };

    match crate::core::execute_download_and_ingest(url.to_string(), target_filename, profile_name, config, None).await {
        Ok(msg) => json!({ "content": [{"type": "text", "text": msg }] }),
        Err(e) => {
            logging::error(format!("MCP download execution failed: {}", e));
            json!({ "isError": true, "content": [{"type": "text", "text": format!("Execution failed: {}", e)}] })
        }
        ,
    }
}

fn build_mcp_target_name(args: &Value) -> String {
    let email_component = args
        .get("email")
        .and_then(Value::as_str)
        .map(sanitize_filename)
        .filter(|value| !value.is_empty());
    let order_component = args
        .get("order")
        .and_then(Value::as_i64)
        .filter(|value| *value >= 0)
        .map(|value| format!("{:04}", value));

    match (email_component, order_component) {
        (Some(email), Some(order)) => format!("{}_{}", order, email),
        (Some(email), None) => email,
        (None, Some(order)) => order,
        (None, None) => "download".to_string(),
    }
}

fn sanitize_filename(input: &str) -> String {
    input
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
}
