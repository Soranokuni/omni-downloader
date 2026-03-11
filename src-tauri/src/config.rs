use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use anyhow::{Context, Result};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EncodingProfile {
    pub name: String,
    pub extension: String,
    pub ffmpeg_args: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppConfig {
    #[serde(default = "default_nas_retention_path")]
    pub nas_retention_path: String,
    #[serde(default = "default_binaries_path")]
    pub binaries_path: String,
    #[serde(default = "default_output_path")]
    pub output_path: String,
    #[serde(default = "default_profile_name")]
    pub default_profile: String,
    #[serde(default = "default_profiles")]
    pub profiles: Vec<EncodingProfile>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            nas_retention_path: default_nas_retention_path(),
            binaries_path: default_binaries_path(),
            output_path: default_output_path(),
            default_profile: default_profile_name(),
            profiles: default_profiles(),
        }
    }
}

fn preferred_app_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
            return PathBuf::from(local_app_data).join("Omni Downloader");
        }
    }

    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join(".local").join("share").join("omni-downloader");
    }

    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("omni-downloader-data")
}

pub fn app_data_dir() -> PathBuf {
    preferred_app_dir()
}

fn preferred_downloads_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        if let Ok(user_profile) = env::var("USERPROFILE") {
            return PathBuf::from(user_profile).join("Downloads").join("Omni Downloader");
        }
    }

    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join("Downloads").join("omni-downloader");
    }

    preferred_app_dir().join("downloads")
}

fn default_nas_retention_path() -> String {
    preferred_app_dir().join("nas_storage").to_string_lossy().to_string()
}

fn default_binaries_path() -> String {
    preferred_app_dir().join("binaries").to_string_lossy().to_string()
}

fn default_output_path() -> String {
    preferred_downloads_dir().to_string_lossy().to_string()
}

fn default_profile_name() -> String {
    "Dalet XDCAM 50Mbps".to_string()
}

fn default_profiles() -> Vec<EncodingProfile> {
    vec![
        EncodingProfile {
            name: "Dalet XDCAM 50Mbps".to_string(),
            extension: "mxf".to_string(),
            ffmpeg_args: vec![
                "-filter_complex".to_string(),
                "[0:v]scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2:black,format=yuv422p,setfield=tff[vout]; [0:a]pan=mono|c0=FL[a1]; [0:a]pan=mono|c0=FR[a2]; anullsrc=r=48000:cl=mono[silence]; [silence]asplit=6[a3][a4][a5][a6][a7][a8]".to_string(),
                "-map".to_string(), "[vout]".to_string(), "-map".to_string(), "[a1]".to_string(), "-map".to_string(), "[a2]".to_string(), "-map".to_string(), "[a3]".to_string(), "-map".to_string(), "[a4]".to_string(), "-map".to_string(), "[a5]".to_string(), "-map".to_string(), "[a6]".to_string(), "-map".to_string(), "[a7]".to_string(), "-map".to_string(), "[a8]".to_string(),
                "-c:v".to_string(), "mpeg2video".to_string(), "-b:v".to_string(), "50000k".to_string(), "-minrate".to_string(), "50000k".to_string(), "-maxrate".to_string(), "50000k".to_string(), "-bufsize".to_string(), "17825792".to_string(), "-g".to_string(), "12".to_string(), "-flags".to_string(), "+ildct+ilme".to_string(), "-top".to_string(), "1".to_string(), "-vtag".to_string(), "xd5c".to_string(), "-non_linear_quant".to_string(), "1".to_string(), "-dc".to_string(), "10".to_string(), "-intra_vlc".to_string(), "1".to_string(), "-qmin".to_string(), "1".to_string(), "-qmax".to_string(), "3".to_string(), "-lmin".to_string(), "1*QP2LAMBDA".to_string(),
                "-c:a".to_string(), "pcm_s24le".to_string(), "-ar".to_string(), "48000".to_string(), "-shortest".to_string(),
            ],
        },
        EncodingProfile {
            name: "H.264 1080p MP4".to_string(),
            extension: "mp4".to_string(),
            ffmpeg_args: vec![
                "-vf".to_string(), "scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2".to_string(),
                "-c:v".to_string(), "libx264".to_string(),
                "-preset".to_string(), "fast".to_string(),
                "-crf".to_string(), "23".to_string(),
                "-c:a".to_string(), "aac".to_string(),
                "-b:a".to_string(), "192k".to_string(),
            ],
        },
        EncodingProfile {
            name: "H.265 1080p MP4".to_string(),
            extension: "mp4".to_string(),
            ffmpeg_args: vec![
                "-vf".to_string(), "scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2".to_string(),
                "-c:v".to_string(), "libx265".to_string(),
                "-preset".to_string(), "medium".to_string(),
                "-crf".to_string(), "25".to_string(),
                "-tag:v".to_string(), "hvc1".to_string(),
                "-c:a".to_string(), "aac".to_string(),
                "-b:a".to_string(), "192k".to_string(),
            ],
        },
    ]
}

fn config_path() -> PathBuf {
    let preferred = preferred_app_dir().join("omni-downloader.cfg");
    if preferred.exists() {
        return preferred;
    }

    let legacy_paths = [
        PathBuf::from("omni-downloader.cfg"),
        PathBuf::from("src-tauri").join("omni-downloader.cfg"),
    ];

    for candidate in legacy_paths {
        if candidate.exists() {
            return candidate;
        }
    }

    preferred
}

pub fn load_config() -> Result<AppConfig> {
    let cfg_path = config_path();
    if let Some(parent) = cfg_path.parent() {
        fs::create_dir_all(parent).ok();
    }

    if !cfg_path.exists() {
        let default_config = AppConfig::default();
        let json = serde_json::to_string_pretty(&default_config)?;
        fs::write(&cfg_path, json).context("Failed to write default config")?;
        fs::create_dir_all(&default_config.nas_retention_path).ok();
        fs::create_dir_all(&default_config.binaries_path).ok();
        fs::create_dir_all(&default_config.output_path).ok();
        return Ok(default_config);
    }

    let contents = fs::read_to_string(&cfg_path).context("Failed to read config file")?;
    let mut config: AppConfig = serde_json::from_str(&contents).context("Failed to parse config file")?;
    let mut changed = false;

    if config.profiles.is_empty() {
        config.profiles = default_profiles();
        changed = true;
    }
    if config.default_profile.trim().is_empty() {
        config.default_profile = default_profile_name();
        changed = true;
    }

    changed |= migrate_builtin_profiles(&mut config);

    changed |= normalize_runtime_paths(&mut config);
    
    fs::create_dir_all(&config.nas_retention_path).ok();
    fs::create_dir_all(&config.binaries_path).ok();
    fs::create_dir_all(&config.output_path).ok();

    if changed {
        save_config(&config)?;
    }

    Ok(config)
}

pub fn save_config(config: &AppConfig) -> Result<()> {
    let cfg_path = config_path();
    if let Some(parent) = cfg_path.parent() {
        fs::create_dir_all(parent).ok();
    }

    fs::create_dir_all(&config.nas_retention_path).ok();
    fs::create_dir_all(&config.binaries_path).ok();
    fs::create_dir_all(&config.output_path).ok();

    let json = serde_json::to_string_pretty(config)?;
    fs::write(&cfg_path, json).context("Failed to write config")?;
    Ok(())
}

fn migrate_builtin_profiles(config: &mut AppConfig) -> bool {
    let mut changed = false;

    if let Some(profile) = config
        .profiles
        .iter_mut()
        .find(|profile| profile.name == "Dalet XDCAM 50Mbps")
    {
        let uses_legacy_silence = profile
            .ffmpeg_args
            .iter()
            .any(|argument| argument.contains("aevalsrc=0:d=1"));
        let missing_shortest = !profile.ffmpeg_args.iter().any(|argument| argument == "-shortest");

        if uses_legacy_silence || missing_shortest {
            if let Some(default_profile) = default_profiles()
                .into_iter()
                .find(|candidate| candidate.name == profile.name)
            {
                *profile = default_profile;
                changed = true;
            }
        }
    }

    changed
}

fn normalize_runtime_paths(config: &mut AppConfig) -> bool {
    let mut changed = false;

    let normalized_retention = normalize_runtime_path(&config.nas_retention_path, default_nas_retention_path);
    if normalized_retention != config.nas_retention_path {
        config.nas_retention_path = normalized_retention;
        changed = true;
    }

    let normalized_binaries = normalize_runtime_path(&config.binaries_path, default_binaries_path);
    if normalized_binaries != config.binaries_path {
        config.binaries_path = normalized_binaries;
        changed = true;
    }

    let normalized_output = normalize_runtime_path(&config.output_path, default_output_path);
    if normalized_output != config.output_path {
        config.output_path = normalized_output;
        changed = true;
    }

    changed
}

fn normalize_runtime_path(path: &str, fallback: fn() -> String) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return fallback();
    }

    let candidate = PathBuf::from(trimmed);
    if candidate.is_absolute() {
        return candidate.to_string_lossy().to_string();
    }

    let mut resolved = app_data_dir();
    for component in Path::new(trimmed).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {}
            Component::Normal(part) => resolved.push(part),
            Component::Prefix(_) | Component::RootDir => {}
        }
    }

    resolved.to_string_lossy().to_string()
}
