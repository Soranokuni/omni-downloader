use crate::binaries;
use crate::config::{AppConfig, EncodingProfile};
use crate::logging;
use anyhow::{Context, Result};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

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

#[derive(Clone)]
pub struct ExecutionEmitters {
    pub progress: Arc<dyn Fn(ProgressPayload) + Send + Sync>,
    pub log: Arc<dyn Fn(LogPayload) + Send + Sync>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DownloadExecution {
    pub exported_files: Vec<String>,
}

impl DownloadExecution {
    pub fn summary(&self) -> String {
        format!(
            "Successfully exported {} file(s):\n{}",
            self.exported_files.len(),
            self.exported_files.join("\n")
        )
    }
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

pub async fn execute_download_and_ingest(
    url: String,
    target_filename: String,
    profile_name: String,
    config: &AppConfig,
    emitters: Option<ExecutionEmitters>,
) -> Result<DownloadExecution> {
    let id = xid::new().to_string();

    let emit_progress = |status: &str, progress: f64| {
        if let Some(emitters) = emitters.as_ref() {
            (emitters.progress)(ProgressPayload {
                id: id.clone(),
                status: status.to_string(),
                progress,
            });
        }
    };

    let emit_log = |msg: &str| {
        logging::info(msg);
        if let Some(emitters) = emitters.as_ref() {
            (emitters.log)(LogPayload {
                message: msg.to_string(),
            });
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

    Ok(DownloadExecution {
        exported_files: exported_files
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
    })
}

async fn resolve_media_urls(
    input_url: &str,
    yt_dlp_path: &Path,
) -> Result<Vec<MediaResolution>> {
    if let Ok(probe) = probe_with_ytdlp(yt_dlp_path, input_url) {
        return Ok(vec![MediaResolution {
            download_url: input_url.to_string(),
            suggested_title: probe.title,
            extractor: probe.extractor,
            source: "yt-dlp-probe".to_string(),
        }]);
    }

    if looks_like_direct_media_url(input_url) {
        return Ok(vec![MediaResolution {
            download_url: input_url.to_string(),
            suggested_title: None,
            extractor: Some("direct-url".to_string()),
            source: "direct-media-url".to_string(),
        }]);
    }

    anyhow::bail!(
        "yt-dlp could not resolve the provided URL and it does not look like a direct media asset"
    )
}

fn first_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(key).and_then(Value::as_str).map(str::to_string))
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
    [".mp4", ".m3u8", ".mp3", ".mov", ".webm", ".m4a", ".mpd"]
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
