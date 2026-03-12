use crate::config::AppConfig;
use crate::logging;
use anyhow::Result;
use std::fs::{self, File};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tokio::time;

const WINDOWS_FFMPEG_URL: &str = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip";
const WINDOWS_YTDLP_URL: &str = "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download/yt-dlp.exe";
const LINUX_YTDLP_URL: &str = "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download/yt-dlp_linux";
const MIN_YTDLP_SIZE_BYTES: u64 = 1_000_000;
const MIN_FFMPEG_SIZE_BYTES: u64 = 10_000_000;

#[derive(Debug, Clone)]
pub struct RuntimeBinaries {
    pub yt_dlp: PathBuf,
    pub ffmpeg: PathBuf,
}

pub async fn ensure_binaries(config: &AppConfig) -> Result<()> {
    let bin_dir = Path::new(&config.binaries_path);
    if !bin_dir.exists() {
        fs::create_dir_all(bin_dir)?;
    }

    let yt_dlp_path = bin_dir.join(yt_dlp_file_name());
    if !is_valid_ytdlp_binary(&yt_dlp_path) {
        logging::info("Downloading yt-dlp nightly...");
        download_file(ytdlp_download_url(), &yt_dlp_path).await?;
        make_executable(&yt_dlp_path)?;
        if !is_valid_ytdlp_binary(&yt_dlp_path) {
            anyhow::bail!("Downloaded yt-dlp binary is invalid: {}", yt_dlp_path.display());
        }
        logging::info("yt-dlp downloaded.");
    }

    let ffmpeg_path = bin_dir.join(ffmpeg_file_name());
    if !is_valid_ffmpeg_binary(&ffmpeg_path) {
        if let Some(ffmpeg_url) = ffmpeg_download_url() {
            logging::info("Downloading FFmpeg...");
            let zip_path = bin_dir.join("ffmpeg.zip");
            download_file(ffmpeg_url, &zip_path).await?;
            logging::info("Extracting FFmpeg...");
            extract_ffmpeg(&zip_path, bin_dir)?;
            fs::remove_file(zip_path).ok();
            if !is_valid_ffmpeg_binary(&ffmpeg_path) {
                anyhow::bail!("Downloaded ffmpeg binary is invalid: {}", ffmpeg_path.display());
            }
            logging::info("FFmpeg ready.");
        } else if system_command_works("ffmpeg", "-version") {
            logging::info("Using system ffmpeg; no bundled auto-download is configured for this platform.");
        } else {
            anyhow::bail!(
                "No working ffmpeg executable found and bundled auto-download is unavailable on this platform."
            );
        }
    }

    Ok(())
}

pub fn resolve_runtime_binaries(config: &AppConfig) -> Result<RuntimeBinaries> {
    let bundled_ytdlp = Path::new(&config.binaries_path).join(yt_dlp_file_name());
    let bundled_ffmpeg = Path::new(&config.binaries_path).join(ffmpeg_file_name());

    let yt_dlp = if is_valid_ytdlp_binary(&bundled_ytdlp) {
        bundled_ytdlp
    } else if system_command_works("yt-dlp", "--version") {
        PathBuf::from("yt-dlp")
    } else {
        anyhow::bail!(
            "No working yt-dlp executable found. Bundled binary is missing or invalid and no system yt-dlp is available."
        );
    };

    let ffmpeg = if is_valid_ffmpeg_binary(&bundled_ffmpeg) {
        bundled_ffmpeg
    } else if system_command_works("ffmpeg", "-version") {
        PathBuf::from("ffmpeg")
    } else {
        anyhow::bail!(
            "No working ffmpeg executable found. Bundled binary is missing or invalid and no system ffmpeg is available."
        );
    };

    Ok(RuntimeBinaries { yt_dlp, ffmpeg })
}

async fn download_file(url: &str, dest: &PathBuf) -> Result<()> {
    let response = reqwest::get(url).await?.error_for_status()?.bytes().await?;
    let temp_dest = dest.with_extension("download");
    let mut file = File::create(&temp_dest)?;
    file.write_all(&response)?;
    file.flush()?;
    if fs::rename(&temp_dest, dest).is_err() {
        fs::copy(&temp_dest, dest)?;
        fs::remove_file(&temp_dest).ok();
    }
    Ok(())
}

fn extract_ffmpeg(zip_path: &PathBuf, dest_dir: &Path) -> Result<()> {
    let file = File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name().to_string();
        if name.ends_with("ffmpeg.exe") || name.ends_with("ffprobe.exe") {
            let file_name = Path::new(&name).file_name().unwrap();
            let outpath = dest_dir.join(file_name);
            let mut outfile = File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
            outfile.flush()?;
        }
    }

    Ok(())
}

fn is_valid_ytdlp_binary(path: &Path) -> bool {
    is_reasonable_size(path, MIN_YTDLP_SIZE_BYTES) && command_path_works(path, "--version")
}

fn is_valid_ffmpeg_binary(path: &Path) -> bool {
    is_reasonable_size(path, MIN_FFMPEG_SIZE_BYTES) && command_path_works(path, "-version")
}

fn is_reasonable_size(path: &Path, minimum_size: u64) -> bool {
    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.len() >= minimum_size)
        .unwrap_or(false)
}

fn command_path_works(path: &Path, arg: &str) -> bool {
    Command::new(path)
        .arg(arg)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn system_command_works(command: &str, arg: &str) -> bool {
    Command::new(command)
        .arg(arg)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn spawn_ytdlp_updater(config: AppConfig) {
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(3 * 24 * 3600));
        interval.tick().await;

        loop {
            interval.tick().await;
            let Ok(runtime_binaries) = resolve_runtime_binaries(&config) else {
                logging::error("Skipping yt-dlp auto-update because no working binary is currently available.");
                continue;
            };

            logging::info(format!(
                "Running yt-dlp nightly auto-update using {}...",
                runtime_binaries.yt_dlp.display()
            ));
            let _ = Command::new(&runtime_binaries.yt_dlp)
                .arg("--update-to")
                .arg("nightly")
                .output();
        }
    });
}

fn yt_dlp_file_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    }
}

fn ffmpeg_file_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

fn ytdlp_download_url() -> &'static str {
    if cfg!(target_os = "windows") {
        WINDOWS_YTDLP_URL
    } else {
        LINUX_YTDLP_URL
    }
}

fn ffmpeg_download_url() -> Option<&'static str> {
    if cfg!(target_os = "windows") {
        Some(WINDOWS_FFMPEG_URL)
    } else {
        None
    }
}

fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)?;
    }

    #[cfg(not(unix))]
    {
        let _ = path;
    }

    Ok(())
}
