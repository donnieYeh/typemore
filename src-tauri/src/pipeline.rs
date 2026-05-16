use std::{fs, path::PathBuf};

use anyhow::{Context, Result};

use crate::{
    asr::{self, AsrOutput},
    config::AppConfig,
    deepseek, delivery,
    events::DeliveryMode,
    pinyin_hint,
};

#[derive(Debug, Clone)]
pub struct PipelineInput {
    pub wav_path: PathBuf,
}

pub async fn transcribe_local(input: &PipelineInput, config: &AppConfig) -> Result<AsrOutput> {
    asr::transcribe(
        &config.whisper_sidecar_path,
        &config.whisper_model_path,
        input.wav_path.as_path(),
        &config.language,
    )
    .await
}

pub async fn repair_text(config: &AppConfig, transcript: &AsrOutput) -> Result<(String, String)> {
    let pinyin_hint = pinyin_hint::build_pinyin_hint(&transcript.text);
    let final_text = if should_skip_llm(&transcript.text) {
        transcript.text.trim().to_string()
    } else {
        deepseek::repair_text(config, transcript, &pinyin_hint).await?
    };
    Ok((pinyin_hint, final_text))
}

pub fn deliver_text(config: &AppConfig, final_text: &str) -> Result<DeliveryMode> {
    delivery::deliver_text(final_text, config.auto_paste)
}

pub fn cleanup_temp_file(path: &PathBuf) {
    fs::remove_file(path)
        .with_context(|| format!("failed to remove temp wav {}", path.display()))
        .ok();
}

fn should_skip_llm(text: &str) -> bool {
    text.chars()
        .filter(|ch| !ch.is_whitespace() && !matches!(ch, '，' | '。' | '！' | '？' | ',' | '.' | '!' | '?'))
        .count()
        <= 3
}
