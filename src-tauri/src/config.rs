use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AsrProfile {
    Speed,
    Standard,
}

impl Default for AsrProfile {
    fn default() -> Self {
        Self::Standard
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub deepseek_api_key: String,
    pub microphone_device_id: Option<String>,
    pub whisper_model_path: String,
    pub whisper_speed_model_path: String,
    pub whisper_sidecar_path: String,
    pub asr_profile: AsrProfile,
    pub language: String,
    pub auto_paste: bool,
    pub hotkey_buffer_ms: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            deepseek_api_key: String::new(),
            microphone_device_id: None,
            whisper_model_path: String::new(),
            whisper_speed_model_path: String::new(),
            whisper_sidecar_path: String::new(),
            asr_profile: AsrProfile::Standard,
            language: "zh".into(),
            auto_paste: true,
            hotkey_buffer_ms: 300,
        }
    }
}

impl AppConfig {
    pub fn load(config_path: &Path) -> Result<Self> {
        if !config_path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(config_path)
            .with_context(|| format!("failed to read config file {}", config_path.display()))?;
        Ok(serde_json::from_str(&raw).context("failed to parse config file")?)
    }

    pub fn save(&self, config_path: &Path) -> Result<()> {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create config dir {}", parent.display()))?;
        }

        let body = serde_json::to_string_pretty(self).context("failed to serialize config")?;
        fs::write(config_path, body)
            .with_context(|| format!("failed to write config file {}", config_path.display()))
    }
}

pub fn app_data_dir() -> Result<PathBuf> {
    let base = dirs::data_dir().context("missing data dir")?;
    Ok(base.join("typemore"))
}

pub fn config_path() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("config.json"))
}

pub fn logs_dir() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("logs"))
}

pub fn temp_dir() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("temp"))
}

pub fn resources_dir() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("resources"))
}

pub fn bundled_whisper_cli_path() -> Result<PathBuf> {
    Ok(resources_dir()?.join("whisper.cpp").join("whisper-cli.exe"))
}

pub fn bundled_whisper_model_path() -> Result<PathBuf> {
    Ok(resources_dir()?.join("models").join("ggml-small.bin"))
}

pub fn bundled_whisper_speed_model_path() -> Result<PathBuf> {
    Ok(resources_dir()?.join("models").join("ggml-base.bin"))
}
