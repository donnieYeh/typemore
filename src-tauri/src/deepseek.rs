use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{asr::AsrOutput, config::{AppConfig, PromptStyle}};

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    stream: bool,
    messages: Vec<Message>,
}

#[derive(Debug, Serialize)]
struct Message {
    role: &'static str,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: String,
}

pub async fn repair_text(config: &AppConfig, transcript: &AsrOutput, pinyin_hint: &str) -> Result<String> {
    if config.deepseek_api_key.trim().is_empty() {
        return Err(anyhow!("deepseek api key is empty"));
    }

    let client = Client::new();
    let request = ChatRequest {
        model: "deepseek-v4-flash",
        stream: false,
        messages: vec![
            Message {
                role: "system",
                content: build_system_prompt(&config.prompt_style),
            },
            Message {
                role: "user",
                content: build_user_prompt(transcript, pinyin_hint),
            },
        ],
    };

    let response = client
        .post("https://api.deepseek.com/chat/completions")
        .bearer_auth(&config.deepseek_api_key)
        .json(&request)
        .send()
        .await
        .context("failed to call deepseek")?
        .error_for_status()
        .context("deepseek returned an error status")?;

    let payload: ChatResponse = response.json().await.context("failed to parse deepseek response")?;
    payload
        .choices
        .first()
        .map(|choice| choice.message.content.trim().to_string())
        .filter(|value| !value.is_empty())
        .context("deepseek returned empty content")
}

fn build_system_prompt(style: &PromptStyle) -> String {
    match style {
        PromptStyle::Default => [
            "你负责修复中文语音转写结果。",
            "请结合原始转写和拼音提示，修正同音字、缺字、错字、标点和断句问题。",
            "删除明显的口头禅、重复、自我修正和转写噪声。",
            "保持原意，不要编造事实。",
            "无论输入是简体还是繁体，最终输出必须统一为简体中文。",
            "只返回最终可直接粘贴的文本，不要解释。",
        ].join(" "),
        PromptStyle::Concise => [
            "你负责修复中文语音转写结果。",
            "修正同音字、缺字、错字、标点问题。",
            "删除口头禅、重复和噪声。",
            "保持简洁，不要扩展内容。",
            "只返回最终可直接粘贴的文本。",
        ].join(" "),
        PromptStyle::Formal => [
            "你负责将中文语音转写结果整理为正式文本。",
            "修正所有文字错误、标点和断句。",
            "删除口语化表达、重复和口头禅。",
            "将内容整理为流畅的正式书面语。",
            "保持原意，不添加内容。",
            "只返回最终可直接粘贴的文本。",
        ].join(" "),
        PromptStyle::Creative => [
            "你负责润色中文语音转写结果。",
            "在保持原意的基础上，让文字更加生动流畅。",
            "修正同音字、错字和标点。",
            "适当优化表达，使其更具表现力。",
            "只返回最终可直接粘贴的文本。",
        ].join(" "),
    }
}

fn build_user_prompt(transcript: &AsrOutput, pinyin_hint: &str) -> String {
    let segments = if transcript.segments.is_empty() {
        "无分段信息。".to_string()
    } else {
        transcript
            .segments
            .iter()
            .map(|segment| format!("[{}-{}] {}", segment.start_ms, segment.end_ms, segment.text))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "请输出最终润色后的文本，并统一使用简体中文。\n\n原始转写：\n{}\n\n拼音提示：\n{}\n\n分段信息：\n{}\n",
        transcript.text, pinyin_hint, segments
    )
}