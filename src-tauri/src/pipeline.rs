use std::{fs, path::PathBuf};

use anyhow::{Context, Result};

use crate::{
    asr::AsrOutput,
    bootstrap,
    config::AppConfig,
    deepseek,
    delivery,
    events::DeliveryMode,
    hotkey::UserCorrection,
    pinyin_hint,
    state::RuntimeState,
};

#[derive(Debug, Clone)]
pub struct PipelineInput {
    pub wav_path: PathBuf,
}

pub async fn transcribe_local(
    input: &PipelineInput,
    config: &AppConfig,
    state: &RuntimeState,
) -> Result<AsrOutput> {
    let worker = state.asr_worker().await;
    worker
        .transcribe(
            &config.whisper_sidecar_path,
            bootstrap::resolve_model_path(config)?,
            input.wav_path.as_path(),
            &config.language,
        )
        .await
}

pub async fn repair_text(
    config: &AppConfig,
    transcript: &AsrOutput,
    dissatisfaction: bool,
    corrections: &[UserCorrection],
) -> Result<(String, String)> {
    let pinyin_hint = pinyin_hint::build_pinyin_hint(&transcript.text);
    let cursor_context = delivery::get_cursor_context().unwrap_or_default();
    let final_text = if should_skip_llm(&transcript.text) {
        transcript.text.trim().to_string()
    } else {
        deepseek::repair_text(config, transcript, &pinyin_hint, &cursor_context, dissatisfaction, corrections).await?
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
    let effective_chars = text
        .chars()
        .filter(|ch| !ch.is_whitespace() && !matches!(ch, ',' | '.' | '!' | '?' | ';' | ':'))
        .count();
    let contains_han = text.chars().any(is_cjk_unified_ideograph);

    effective_chars <= 3 && !contains_han
}

fn is_cjk_unified_ideograph(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2EBEF
    )
}
