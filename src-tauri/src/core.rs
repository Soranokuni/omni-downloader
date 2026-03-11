use crate::binaries;
use crate::config::{AppConfig, EncodingProfile};
use crate::logging;
use anyhow::{Context, Result};
use reqwest::Url;
use scraper::{Html, Selector};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::Emitter;

#[derive(Clone, serde::Serialize)]
pub struct ProgressPayload {
    pub id: String,
    pub status: String,
    pub progress: f64,
}

#[derive(Clone, serde::Serialize)]
pub struct LogPayload {
    pub message: String,
}

#[derive(Debug, Clone)]
struct MediaProbe {
    title: Option<String>,
    extractor: Option<String>,
}

#[derive(Debug, Clone)]
struct MediaResolution {
    download_url: String,
    suggested_title: Option<String>,
    extractor: Option<String>,
    source: String,
}

#[derive(Debug, Clone)]
struct MediaCandidate {
    download_url: String,
    suggested_title: Option<String>,
    position: usize,
    source: String,
}

#[derive(Debug, Clone)]
struct DiscoveredMedia {
    page_title: Option<String>,
    primary_candidates: Vec<MediaCandidate>,
    secondary_candidates: Vec<MediaCandidate>,
}

#[derive(Debug, Clone)]
struct GlomexEmbed {
    integration_id: String,
    playlist_id: String,
    position: usize,
}

pub async fn execute_download_and_ingest(
    url: String,
    target_filename: String,
    profile_name: String,
    config: &AppConfig,
    app_handle: Option<tauri::AppHandle>,
) -> Result<String> {
    let id = xid::new().to_string();

    let emit_progress = |status: &str, progress: f64| {
        if let Some(app) = &app_handle {
            let _ = app.emit(
                "download-progress",
                ProgressPayload {
                    id: id.clone(),
                    status: status.to_string(),
                    progress,
                },
            );
        }
    };

    let emit_log = |msg: &str| {
        logging::info(msg);
        if let Some(app) = &app_handle {
            let _ = app.emit(
                "backend-log",
                LogPayload {
                    message: msg.to_string(),
                },
            );
        }
    };

    let profile = config
        .profiles
        .iter()
        .find(|candidate| candidate.name == profile_name)
        .cloned()
        .or_else(|| config.profiles.first().cloned())
        .unwrap_or_else(default_profile);

    let runtime_binaries = binaries::resolve_runtime_binaries(config)?;
    emit_log(&format!(
        "[system] Using yt-dlp binary: {}",
        runtime_binaries.yt_dlp.display()
    ));
    emit_log(&format!(
        "[system] Using ffmpeg binary: {}",
        runtime_binaries.ffmpeg.display()
    ));

    let resolutions = resolve_media_urls(&url, &runtime_binaries.yt_dlp).await?;
    emit_log(&format!(
        "[resolver] Resolved {} relevant media item(s) from input URL.",
        resolutions.len()
    ));

    let output_dir = Path::new(&config.output_path);
    fs::create_dir_all(output_dir).ok();

    let total_items = resolutions.len();
    let mut exported_files = Vec::new();

    for (index, resolution) in resolutions.iter().enumerate() {
        let item_number = index + 1;
        let work_dir = std::env::temp_dir()
            .join("omni-downloader")
            .join(&id)
            .join(format!("item-{:02}", item_number));
        fs::create_dir_all(&work_dir)?;
        let yt_output_template = work_dir.join("downloaded.%(ext)s");

        emit_progress("downloading", progress_value(index, total_items, 0.05));
        emit_log(&format!(
            "[resolver] Item {}/{} source={} extractor={} url={}",
            item_number,
            total_items,
            resolution.source,
            resolution.extractor.as_deref().unwrap_or("unknown"),
            resolution.download_url
        ));
        emit_log(&format!(
            "[yt-dlp] Starting download for item {}/{}: {}",
            item_number,
            total_items,
            resolution.download_url
        ));

        let yt_output = Command::new(&runtime_binaries.yt_dlp)
            .arg("--no-playlist")
            .arg("--newline")
            .arg("-o")
            .arg(&yt_output_template)
            .arg(&resolution.download_url)
            .output()
            .context("Failed to launch yt-dlp")?;

        if !yt_output.status.success() {
            emit_progress("error", 0.0);
            log_command_output("[yt-dlp]", &yt_output.stdout, &yt_output.stderr, &emit_log);
            logging::error(format!("yt-dlp download failed for item {}", item_number));
            cleanup_workspace(&work_dir);
            anyhow::bail!("yt-dlp download failed for item {}", item_number);
        }

        log_command_output("[yt-dlp]", &yt_output.stdout, &yt_output.stderr, &emit_log);
        emit_progress("downloading", progress_value(index, total_items, 0.45));
        emit_log(&format!("[system] Download complete for item {}/{}. Locating file...", item_number, total_items));

        let downloaded_file = locate_downloaded_file(&work_dir)?;

        let final_base_name = build_output_stem(
            &target_filename,
            resolution.suggested_title.as_deref(),
            &id,
            index,
            total_items,
        );

        let nas_dir = Path::new(&config.nas_retention_path);
        fs::create_dir_all(nas_dir).ok();
        let nas_original = nas_dir.join(build_retention_filename(
            &final_base_name,
            downloaded_file.extension().and_then(|value| value.to_str()),
        ));
        if let Err(error) = fs::copy(&downloaded_file, &nas_original) {
            emit_log(&format!(
                "[system] Failed to copy source asset to retention storage: {}",
                error
            ));
        } else {
            emit_log(&format!(
                "[system] Copied original file to retention storage: {}",
                nas_original.display()
            ));
        }

        emit_progress("transcoding", progress_value(index, total_items, 0.55));
        emit_log(&format!(
            "[ffmpeg] Starting transcode for item {}/{} using profile: {}",
            item_number,
            total_items,
            profile.name
        ));

        let final_filename = ensure_profile_extension(&final_base_name, &profile.extension);
        let out_file = output_dir.join(&final_filename);

        let mut ffmpeg_cmd = Command::new(&runtime_binaries.ffmpeg);
        ffmpeg_cmd.arg("-y").arg("-i").arg(&downloaded_file);
        for argument in &profile.ffmpeg_args {
            ffmpeg_cmd.arg(argument);
        }
        ffmpeg_cmd.arg(&out_file);

        let ffmpeg_output = ffmpeg_cmd
            .output()
            .context("Failed to launch ffmpeg")?;

        let _ = fs::remove_file(&downloaded_file);
        cleanup_workspace(&work_dir);

        if !ffmpeg_output.status.success() {
            emit_progress("error", 0.0);
            log_command_output("[ffmpeg]", &ffmpeg_output.stdout, &ffmpeg_output.stderr, &emit_log);
            logging::error(format!("ffmpeg transcoding failed for item {}", item_number));
            anyhow::bail!("FFmpeg transcoding failed for item {}", item_number);
        }

        log_command_output("[ffmpeg]", &ffmpeg_output.stdout, &ffmpeg_output.stderr, &emit_log);
        emit_progress("transcoding", progress_value(index, total_items, 0.95));
        emit_log(&format!("[system] Exported file: {}", out_file.display()));
        exported_files.push(out_file);
    }

    emit_progress("done", 100.0);

    let summary = exported_files
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!(
        "Successfully exported {} file(s):\n{}",
        exported_files.len(),
        summary
    ))
}

async fn resolve_media_urls(
    input_url: &str,
    yt_dlp_path: &Path,
) -> Result<Vec<MediaResolution>> {
    let page_probe = probe_with_ytdlp(yt_dlp_path, input_url).ok();
    let discovered_media = discover_media_candidates(input_url).await.ok();

    if let Some(discovered_media) = discovered_media {
        let primary_resolutions = validate_candidates(
            yt_dlp_path,
            &discovered_media.primary_candidates,
            discovered_media.page_title.clone(),
        );
        if !primary_resolutions.is_empty() {
            return Ok(primary_resolutions);
        }

        if let Some(probe) = &page_probe {
            if !is_generic_page_probe(probe) {
                return Ok(vec![MediaResolution {
                    download_url: input_url.to_string(),
                    suggested_title: probe.title.clone().or(discovered_media.page_title.clone()),
                    extractor: probe.extractor.clone(),
                    source: "page-probe".to_string(),
                }]);
            }
        }

        let secondary_resolutions = validate_candidates(
            yt_dlp_path,
            &discovered_media.secondary_candidates,
            discovered_media.page_title.clone(),
        );
        if !secondary_resolutions.is_empty() {
            return Ok(secondary_resolutions);
        }

        if let Some(probe) = page_probe {
            return Ok(vec![MediaResolution {
                download_url: input_url.to_string(),
                suggested_title: probe.title.or(discovered_media.page_title),
                extractor: probe.extractor,
                source: "page-probe".to_string(),
            }]);
        }
    }

    if let Some(probe) = page_probe {
        return Ok(vec![MediaResolution {
            download_url: input_url.to_string(),
            suggested_title: probe.title,
            extractor: probe.extractor,
            source: "page-probe".to_string(),
        }]);
    }

    anyhow::bail!("Unable to resolve a downloadable media URL from the provided input")
}

fn probe_with_ytdlp(yt_dlp_path: &Path, url: &str) -> Result<MediaProbe> {
    let output = Command::new(yt_dlp_path)
        .arg("--dump-single-json")
        .arg("--skip-download")
        .arg("--no-playlist")
        .arg("--no-warnings")
        .arg(url)
        .output()
        .context("Failed to launch yt-dlp probe")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(stderr.trim().to_string());
    }

    let json: Value = serde_json::from_slice(&output.stdout).context("yt-dlp returned invalid JSON")?;
    Ok(MediaProbe {
        title: first_string(&json, &["title", "fulltitle"]),
        extractor: first_string(&json, &["extractor_key", "extractor"]),
    })
}

async fn discover_media_candidates(input_url: &str) -> Result<DiscoveredMedia> {
    let response = reqwest::get(input_url)
        .await
        .context("Failed to fetch input page")?
        .error_for_status()
        .context("Input page returned an error status")?;
    let html = response.text().await.context("Failed to read input page body")?;
    let (title, mut seen_urls, mut primary_candidates, secondary_candidates, glomex_embeds) = {
        let document = Html::parse_document(&html);
        let base_url = Url::parse(input_url).context("Input URL is invalid")?;

        let title = select_text(&document, "meta[property='og:title']", Some("content"))
            .or_else(|| select_text(&document, "h1", None))
            .or_else(|| select_text(&document, "title", None));

        let mut seen_urls = HashSet::new();
        let mut primary_candidates = Vec::new();
        let mut secondary_candidates = Vec::new();
        let glomex_embeds = collect_glomex_embeds(&document);

        extract_snippet_video_candidates(
            &document,
            &mut primary_candidates,
            &mut seen_urls,
        );
        extract_json_ld_video_candidates(
            &document,
            &base_url,
            &mut primary_candidates,
            &mut seen_urls,
        );

        let candidate_selectors = [
            ("meta[property='og:video']", "content"),
            ("meta[property='og:video:url']", "content"),
            ("meta[name='twitter:player:stream']", "content"),
            ("video", "src"),
            ("video source", "src"),
            ("iframe", "src"),
        ];

        for (selector, attribute) in candidate_selectors {
            let parsed_selector = Selector::parse(selector).unwrap();
            for node in document.select(&parsed_selector) {
                if let Some(value) = node.value().attr(attribute) {
                    if let Ok(candidate) = normalize_candidate_url(&base_url, value) {
                        let position = secondary_candidates.len();
                        push_candidate(
                            &mut secondary_candidates,
                            &mut seen_urls,
                            MediaCandidate {
                                download_url: candidate,
                                suggested_title: title.clone(),
                                position,
                                source: format!("selector:{}", selector),
                            },
                        );
                    }
                }
            }
        }

        (title, seen_urls, primary_candidates, secondary_candidates, glomex_embeds)
    };

    extract_glomex_candidates(
        &glomex_embeds,
        input_url,
        &mut primary_candidates,
        &mut seen_urls,
    ).await;

    Ok(DiscoveredMedia {
        page_title: title,
        primary_candidates,
        secondary_candidates,
    })
}

fn normalize_candidate_url(base_url: &Url, raw: &str) -> Result<String> {
    if raw.trim().is_empty() {
        anyhow::bail!("Empty candidate URL")
    }

    if raw.starts_with("//") {
        return Ok(format!("{}:{}", base_url.scheme(), raw));
    }

    if let Ok(absolute) = Url::parse(raw) {
        return Ok(absolute.to_string());
    }

    Ok(base_url.join(raw)?.to_string())
}

fn first_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(key).and_then(Value::as_str).map(str::to_string))
}

fn validate_candidates(
    yt_dlp_path: &Path,
    candidates: &[MediaCandidate],
    page_title: Option<String>,
) -> Vec<MediaResolution> {
    let mut resolutions = Vec::new();
    for candidate in candidates {
        if let Ok(probe) = probe_with_ytdlp(yt_dlp_path, &candidate.download_url) {
            resolutions.push(MediaResolution {
                download_url: candidate.download_url.clone(),
                suggested_title: candidate
                    .suggested_title
                    .clone()
                    .or(probe.title)
                    .or(page_title.clone()),
                extractor: probe.extractor,
                source: format!("{}#{}", candidate.source, candidate.position + 1),
            });
            continue;
        }

        if looks_like_direct_media_url(&candidate.download_url) {
            resolutions.push(MediaResolution {
                download_url: candidate.download_url.clone(),
                suggested_title: candidate.suggested_title.clone().or(page_title.clone()),
                extractor: Some("html-fallback".to_string()),
                source: format!("{}#{}", candidate.source, candidate.position + 1),
            });
        }
    }

    resolutions
}

fn extract_snippet_video_candidates(
    document: &Html,
    candidates: &mut Vec<MediaCandidate>,
    seen_urls: &mut HashSet<String>,
) {
    let figure_selector = Selector::parse("figure.snippetVideo").unwrap();
    let plugin_selector = Selector::parse("[data-plugin-youtube]").unwrap();
    let image_selector = Selector::parse("img").unwrap();

    for (position, figure) in document.select(&figure_selector).enumerate() {
        for node in figure.select(&plugin_selector) {
            let Some(raw_plugin_value) = node.value().attr("data-plugin-youtube") else {
                continue;
            };

            let plugin_value = raw_plugin_value
                .replace("&quot;", "\"")
                .replace("&#34;", "\"");
            let Some(video_id) = serde_json::from_str::<Value>(&plugin_value)
                .ok()
                .and_then(|value| value.get("ID").and_then(Value::as_str).map(str::to_string)) else {
                continue;
            };

            let figure_title = figure
                .select(&image_selector)
                .next()
                .and_then(|image| image.value().attr("alt"))
                .map(str::to_string);

            push_candidate(
                candidates,
                seen_urls,
                MediaCandidate {
                    download_url: format!("https://www.youtube.com/watch?v={}", video_id),
                    suggested_title: figure_title,
                    position,
                    source: "article-snippet-youtube".to_string(),
                },
            );
        }
    }
}

fn extract_json_ld_video_candidates(
    document: &Html,
    base_url: &Url,
    candidates: &mut Vec<MediaCandidate>,
    seen_urls: &mut HashSet<String>,
) {
    let script_selector = Selector::parse("script[type='application/ld+json']").unwrap();
    for script in document.select(&script_selector) {
        let payload = script.text().collect::<Vec<_>>().join(" ");
        let Ok(value) = serde_json::from_str::<Value>(&payload) else {
            continue;
        };
        collect_video_objects_from_json(&value, base_url, candidates, seen_urls);
    }
}

fn collect_glomex_embeds(document: &Html) -> Vec<GlomexEmbed> {
    let iframe_selector = Selector::parse("iframe[src*='player.glomex.com']").unwrap();
    let mut embeds = Vec::new();

    for (position, iframe) in document.select(&iframe_selector).enumerate() {
        let Some(raw_src) = iframe.value().attr("src") else {
            continue;
        };

        let Ok(src) = Url::parse(raw_src) else {
            continue;
        };

        let integration_id = src
            .query_pairs()
            .find(|(key, _)| key == "integrationId")
            .map(|(_, value)| value.into_owned())
            .filter(|value| !value.is_empty());
        let playlist_id = src
            .query_pairs()
            .find(|(key, _)| key == "playlistId")
            .map(|(_, value)| value.into_owned())
            .filter(|value| !value.is_empty());

        let (Some(integration_id), Some(playlist_id)) = (integration_id, playlist_id) else {
            continue;
        };

        embeds.push(GlomexEmbed {
            integration_id,
            playlist_id,
            position,
        });
    }

    embeds
}

async fn extract_glomex_candidates(
    embeds: &[GlomexEmbed],
    input_url: &str,
    candidates: &mut Vec<MediaCandidate>,
    seen_urls: &mut HashSet<String>,
) {
    for embed in embeds {
        let Ok(api_url) = Url::parse_with_params(
            "https://integration-cloudfront-eu-west-1.mes.glomex.cloud/",
            &[
                ("integration_id", embed.integration_id.as_str()),
                ("playlist_id", embed.playlist_id.as_str()),
                ("current_url", input_url),
            ],
        ) else {
            continue;
        };

        let Ok(response) = reqwest::get(api_url).await else {
            continue;
        };
        let Ok(body) = response.error_for_status() else {
            continue;
        };
        let Ok(payload) = body.text().await else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&payload) else {
            continue;
        };

        let Some(video) = value
            .get("videos")
            .and_then(Value::as_array)
            .and_then(|videos| videos.first()) else {
            continue;
        };

        let download_url = video
            .get("source")
            .and_then(Value::as_object)
            .and_then(|source| {
                source
                    .get("progressive")
                    .and_then(Value::as_str)
                    .or_else(|| source.get("hls").and_then(Value::as_str))
                    .or_else(|| source.get("seo_content_url").and_then(Value::as_str))
            })
            .map(str::to_string);

        let Some(download_url) = download_url else {
            continue;
        };

        let suggested_title = first_string(video, &["title"])
            .or_else(|| {
                video
                    .get("titles")
                    .and_then(|titles| first_string(titles, &["el", "default"]))
            });

        push_candidate(
            candidates,
            seen_urls,
            MediaCandidate {
                download_url,
                suggested_title,
                position: embed.position,
                source: "article-glomex".to_string(),
            },
        );
    }
}

fn collect_video_objects_from_json(
    value: &Value,
    base_url: &Url,
    candidates: &mut Vec<MediaCandidate>,
    seen_urls: &mut HashSet<String>,
) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_video_objects_from_json(item, base_url, candidates, seen_urls);
            }
        }
        Value::Object(_) => {
            if is_video_object(value) {
                if let Some(raw_url) = first_string(value, &["contentUrl", "url", "embedUrl"]) {
                    if let Ok(normalized_url) = normalize_candidate_url(base_url, &raw_url) {
                        let position = value
                            .get("position")
                            .and_then(Value::as_u64)
                            .unwrap_or(candidates.len() as u64) as usize;
                        push_candidate(
                            candidates,
                            seen_urls,
                            MediaCandidate {
                                download_url: normalized_url,
                                suggested_title: first_string(value, &["name", "headline"]),
                                position,
                                source: "json-ld-video".to_string(),
                            },
                        );
                    }
                }
            }

            if let Some(video_value) = value.get("video") {
                collect_video_objects_from_json(video_value, base_url, candidates, seen_urls);
            }

            if let Some(item_list) = value.get("itemListElement") {
                collect_video_objects_from_json(item_list, base_url, candidates, seen_urls);
            }
        }
        _ => {}
    }
}

fn is_video_object(value: &Value) -> bool {
    match value.get("@type") {
        Some(Value::String(type_name)) => type_name.eq_ignore_ascii_case("VideoObject"),
        Some(Value::Array(types)) => types.iter().any(|entry| {
            entry
                .as_str()
                .map(|type_name| type_name.eq_ignore_ascii_case("VideoObject"))
                .unwrap_or(false)
        }),
        _ => false,
    }
}

fn push_candidate(
    candidates: &mut Vec<MediaCandidate>,
    seen_urls: &mut HashSet<String>,
    candidate: MediaCandidate,
) {
    if seen_urls.insert(candidate_dedup_key(&candidate.download_url)) {
        candidates.push(candidate);
    }
}

fn candidate_dedup_key(raw: &str) -> String {
    let trimmed = raw.trim();
    let Ok(mut parsed) = Url::parse(trimmed) else {
        return trimmed.to_string();
    };

    if let Some(video_id) = youtube_video_id(&parsed) {
        return format!("youtube:{}", video_id);
    }

    parsed.set_fragment(None);
    parsed.to_string()
}

fn youtube_video_id(url: &Url) -> Option<String> {
    let host = url.host_str()?.to_ascii_lowercase();
    if matches!(host.as_str(), "youtu.be") {
        return url
            .path_segments()?
            .find(|segment| !segment.is_empty())
            .map(str::to_string);
    }

    if !matches!(
        host.as_str(),
        "youtube.com" | "www.youtube.com" | "m.youtube.com" | "youtube-nocookie.com" | "www.youtube-nocookie.com"
    ) {
        return None;
    }

    if url.path() == "/watch" {
        return url
            .query_pairs()
            .find(|(key, _)| key == "v")
            .map(|(_, value)| value.into_owned())
            .filter(|value| !value.is_empty());
    }

    let mut segments = url.path_segments()?;
    let first = segments.next()?;
    match first {
        "embed" | "shorts" | "live" | "v" => segments.next().map(str::to_string),
        _ => None,
    }
}

fn is_generic_page_probe(probe: &MediaProbe) -> bool {
    matches!(
        probe.extractor.as_deref(),
        Some("HTML5MediaEmbed") | Some("generic") | Some("Generic")
    )
}

fn select_text(document: &Html, selector: &str, attribute: Option<&str>) -> Option<String> {
    let parsed_selector = Selector::parse(selector).ok()?;
    let node = document.select(&parsed_selector).next()?;

    match attribute {
        Some(attribute_name) => node.value().attr(attribute_name).map(str::to_string),
        None => {
            let text = node.text().collect::<Vec<_>>().join(" ");
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
    }
}

fn locate_downloaded_file(work_dir: &Path) -> Result<PathBuf> {
    let entries = fs::read_dir(work_dir).context("Failed to inspect working directory")?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let extension = path.extension().and_then(|value| value.to_str()).unwrap_or("");
            if extension != "part" && extension != "ytdl" {
                return Ok(path);
            }
        }
    }

    anyhow::bail!("Downloaded file not found")
}

fn log_command_output(prefix: &str, stdout: &[u8], stderr: &[u8], emit_log: &dyn Fn(&str)) {
    for line in String::from_utf8_lossy(stdout).lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            emit_log(&format!("{} {}", prefix, trimmed));
        }
    }

    for line in String::from_utf8_lossy(stderr).lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            emit_log(&format!("{} {}", prefix, trimmed));
        }
    }
}

fn build_output_stem(
    provided_name: &str,
    suggested_title: Option<&str>,
    fallback_id: &str,
    index: usize,
    total: usize,
) -> String {
    let candidate = if !provided_name.trim().is_empty() {
        provided_name.trim()
    } else if let Some(title) = suggested_title {
        title.trim()
    } else {
        fallback_id
    };

    let base_name = sanitize_filename(candidate);
    if total > 1 {
        format!("{}_{:02}", base_name, index + 1)
    } else {
        base_name
    }
}

fn sanitize_filename(input: &str) -> String {
    let sanitized = input
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect::<String>();

    let collapsed = sanitized
        .split_whitespace()
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    let trimmed = collapsed.trim_matches('.').trim();
    if trimmed.is_empty() {
        "download".to_string()
    } else {
        trimmed.to_string()
    }
}

fn ensure_profile_extension(base_name: &str, extension: &str) -> String {
    let normalized_extension = extension.trim_start_matches('.');
    if base_name
        .to_ascii_lowercase()
        .ends_with(&format!(".{}", normalized_extension.to_ascii_lowercase()))
    {
        base_name.to_string()
    } else {
        format!("{}.{}", base_name, normalized_extension)
    }
}

fn looks_like_direct_media_url(url: &str) -> bool {
    let lowercase = url.to_ascii_lowercase();
    [".mp4", ".m3u8", ".mp3", ".mov", ".webm"]
        .iter()
        .any(|extension| lowercase.contains(extension))
}

fn cleanup_workspace(work_dir: &Path) {
    fs::remove_dir_all(work_dir).ok();
}

fn build_retention_filename(base_name: &str, extension: Option<&str>) -> String {
    match extension.filter(|value| !value.is_empty()) {
        Some(extension) => format!("{}_source.{}", base_name, extension.trim_start_matches('.')),
        None => format!("{}_source", base_name),
    }
}

fn progress_value(index: usize, total: usize, stage_fraction: f64) -> f64 {
    if total == 0 {
        return 0.0;
    }

    let total = total as f64;
    (((index as f64) + stage_fraction) / total * 100.0).clamp(0.0, 100.0)
}

fn default_profile() -> EncodingProfile {
    EncodingProfile {
        name: "Dalet XDCAM 50Mbps".to_string(),
        extension: "mxf".to_string(),
        ffmpeg_args: vec![
            "-filter_complex".to_string(),
            "[0:v]scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2:black,format=yuv422p,setfield=tff[vout]; [0:a]pan=mono|c0=FL[a1]; [0:a]pan=mono|c0=FR[a2]; anullsrc=r=48000:cl=mono[silence]; [silence]asplit=6[a3][a4][a5][a6][a7][a8]".to_string(),
            "-map".to_string(),
            "[vout]".to_string(),
            "-map".to_string(),
            "[a1]".to_string(),
            "-map".to_string(),
            "[a2]".to_string(),
            "-map".to_string(),
            "[a3]".to_string(),
            "-map".to_string(),
            "[a4]".to_string(),
            "-map".to_string(),
            "[a5]".to_string(),
            "-map".to_string(),
            "[a6]".to_string(),
            "-map".to_string(),
            "[a7]".to_string(),
            "-map".to_string(),
            "[a8]".to_string(),
            "-c:v".to_string(),
            "mpeg2video".to_string(),
            "-b:v".to_string(),
            "50000k".to_string(),
            "-minrate".to_string(),
            "50000k".to_string(),
            "-maxrate".to_string(),
            "50000k".to_string(),
            "-bufsize".to_string(),
            "17825792".to_string(),
            "-g".to_string(),
            "12".to_string(),
            "-flags".to_string(),
            "+ildct+ilme".to_string(),
            "-top".to_string(),
            "1".to_string(),
            "-vtag".to_string(),
            "xd5c".to_string(),
            "-non_linear_quant".to_string(),
            "1".to_string(),
            "-dc".to_string(),
            "10".to_string(),
            "-intra_vlc".to_string(),
            "1".to_string(),
            "-qmin".to_string(),
            "1".to_string(),
            "-qmax".to_string(),
            "3".to_string(),
            "-lmin".to_string(),
            "1*QP2LAMBDA".to_string(),
            "-c:a".to_string(),
            "pcm_s24le".to_string(),
            "-ar".to_string(),
            "48000".to_string(),
            "-shortest".to_string(),
        ],
    }
}
