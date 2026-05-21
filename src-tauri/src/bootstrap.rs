use std::{
    fs::{self, File},
    io::{Cursor, Write},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::Deserialize;
use tokio::task;
use zip::ZipArchive;

use crate::config::{self, AsrProfile};

const WHISPER_RELEASE_API: &str = "https://api.github.com/repos/ggml-org/whisper.cpp/releases/latest";
const WHISPER_ASSET_NAME: &str = "whisper-blas-bin-x64.zip";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WhisperModelSize {
    Tiny,
    Base,
    Small,
    Medium,
    Large,
}

impl WhisperModelSize {
    pub fn display_name(&self) -> &'static str {
        match self {
            WhisperModelSize::Tiny => "tiny",
            WhisperModelSize::Base => "base",
            WhisperModelSize::Small => "small",
            WhisperModelSize::Medium => "medium",
            WhisperModelSize::Large => "large",
        }
    }

    pub fn filename(&self) -> String {
        format!("ggml-{}.bin", self.display_name())
    }

    pub fn url(&self) -> String {
        let name = self.display_name();
        format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{name}.bin")
    }
}

pub const ALL_MODEL_SIZES: &[WhisperModelSize] = &[
    WhisperModelSize::Tiny,
    WhisperModelSize::Base,
    WhisperModelSize::Small,
    WhisperModelSize::Medium,
    WhisperModelSize::Large,
];

#[derive(Debug, Clone, serde::Serialize)]
pub struct BootstrapResult {
    pub whisper_sidecar_path: String,
    pub whisper_model_path: String,
    pub whisper_speed_model_path: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelInfo {
    pub size: String,
    pub path: String,
    pub downloaded: bool,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

pub async fn ensure_resources() -> Result<BootstrapResult> {
    let whisper_path = config::bundled_whisper_cli_path()?;
    let model_path = config::bundled_whisper_model_path()?;
    let speed_model_path = config::bundled_whisper_speed_model_path()?;

    if !whisper_path.exists() {
        download_whisper_cli(&whisper_path).await?;
    }
    if !model_path.exists() {
        download_model(&model_path, &WhisperModelSize::Small).await?;
    }
    if !speed_model_path.exists() {
        download_model(&speed_model_path, &WhisperModelSize::Base).await?;
    }

    Ok(BootstrapResult {
        whisper_sidecar_path: whisper_path.display().to_string(),
        whisper_model_path: model_path.display().to_string(),
        whisper_speed_model_path: speed_model_path.display().to_string(),
    })
}

pub async fn ensure_speed_model() -> Result<String> {
    let speed_model_path = config::bundled_whisper_speed_model_path()?;
    if !speed_model_path.exists() {
        download_model(&speed_model_path, &WhisperModelSize::Base).await?;
    }

    Ok(speed_model_path.display().to_string())
}

pub async fn ensure_model(model_size: WhisperModelSize) -> Result<String> {
    let model_dir = config::resources_dir()?.join("models");
    let model_path = model_dir.join(model_size.filename());

    if !model_path.exists() {
        download_model(&model_path, &model_size).await?;
    }

    Ok(model_path.display().to_string())
}

pub fn list_available_models() -> Vec<ModelInfo> {
    ALL_MODEL_SIZES
        .iter()
        .map(|size| {
            let path = config::resources_dir()
                .map(|d| d.join("models").join(size.filename()))
                .unwrap_or_default();
            ModelInfo {
                size: size.display_name().to_string(),
                path: path.display().to_string(),
                downloaded: path.exists(),
            }
        })
        .collect()
}

pub fn resolve_model_path(config: &crate::config::AppConfig) -> Result<&str> {
    match config.asr_profile {
        AsrProfile::Standard => {
            let path = config.whisper_model_path.trim();
            if path.is_empty() {
                Err(anyhow!("standard model is not configured; download standard resources first"))
            } else if !Path::new(path).exists() {
                Err(anyhow!("standard model path does not exist; download standard resources first"))
            } else {
                Ok(path)
            }
        }
        AsrProfile::Speed => {
            let path = config.whisper_speed_model_path.trim();
            if path.is_empty() {
                Err(anyhow!("speed model is not configured; download it from settings first"))
            } else if !Path::new(path).exists() {
                Err(anyhow!("speed model path does not exist; download it from settings first"))
            } else {
                Ok(path)
            }
        }
    }
}

async fn download_whisper_cli(target_path: &Path) -> Result<()> {
    let client = github_client();
    let release: GithubRelease = client
        .get(WHISPER_RELEASE_API)
        .send()
        .await
        .context("failed to fetch whisper.cpp release metadata")?
        .error_for_status()
        .context("whisper.cpp release metadata request failed")?
        .json()
        .await
        .context("failed to parse whisper.cpp release metadata")?;

    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name == WHISPER_ASSET_NAME)
        .ok_or_else(|| anyhow!("whisper asset {WHISPER_ASSET_NAME} not found in latest release"))?;

    let zip_bytes = client
        .get(asset.browser_download_url)
        .send()
        .await
        .context("failed to download whisper.cpp archive")?
        .error_for_status()
        .context("whisper.cpp archive download failed")?
        .bytes()
        .await
        .context("failed to read whisper.cpp archive bytes")?;

    let target_dir = target_path
        .parent()
        .ok_or_else(|| anyhow!("missing whisper target dir"))?
        .to_path_buf();
    fs::create_dir_all(&target_dir)
        .with_context(|| format!("failed to create {}", target_dir.display()))?;

    let target_path_buf = target_path.to_path_buf();
    task::spawn_blocking(move || unzip_cli_archive(zip_bytes.to_vec(), target_dir, target_path_buf))
        .await
        .context("whisper archive extraction task failed")??;

    Ok(())
}

fn unzip_cli_archive(zip_bytes: Vec<u8>, target_dir: PathBuf, target_path: PathBuf) -> Result<()> {
    let reader = Cursor::new(zip_bytes);
    let mut archive = ZipArchive::new(reader).context("failed to open whisper zip archive")?;
    let mut cli_found = false;

    for index in 0..archive.len() {
        let mut file = archive.by_index(index).context("failed to read zip entry")?;
        let name = file.name().replace('\\', "/");
        let entry_name = Path::new(&name)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();

        if file.is_dir() {
            continue;
        }

        let destination = if entry_name.eq_ignore_ascii_case("whisper-cli.exe") {
            cli_found = true;
            target_path.clone()
        } else {
            target_dir.join(entry_name)
        };

        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let mut output = File::create(&destination)
            .with_context(|| format!("failed to create {}", destination.display()))?;
        std::io::copy(&mut file, &mut output)
            .with_context(|| format!("failed to extract {}", destination.display()))?;
        output.flush().ok();
    }

    if !cli_found {
        return Err(anyhow!("whisper-cli.exe not found in downloaded archive"));
    }

    Ok(())
}

async fn download_model(target_path: &Path, size: &WhisperModelSize) -> Result<()> {
    let client = Client::builder().build().context("failed to build http client")?;
    let url = size.url();
    let bytes = client
        .get(&url)
        .send()
        .await
        .context("failed to download whisper model")?
        .error_for_status()
        .context("whisper model download failed")?
        .bytes()
        .await
        .context("failed to read whisper model bytes")?;

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    fs::write(target_path, bytes).with_context(|| format!("failed to write {}", target_path.display()))
}

fn github_client() -> Client {
    Client::builder()
        .user_agent("typemore/0.1.0")
        .build()
        .expect("github client init failed")
}
