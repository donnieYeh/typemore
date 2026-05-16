use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{asr::AsrOutput, config::AppConfig};

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    stream: bool,
    messages: Vec<Message<'a>>,
}

#[derive(Debug, Serialize)]
struct Message<'a> {
    role: &'a str,
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
                content: concat!(
                    "你是中文语音转写修复助手。",
                    "请基于本地转写原文和拼音提示，修正同音字、漏字、错字、标点和断句，",
                    "并主动清理口语化卡顿、重复词、语气词、自我修正和明显的转写噪音，",
                    "让最终文本表达自然、顺滑、语义清晰。",
                    "如果内容天然包含多个要点、步骤、事项或时间点，请做轻度结构化整理。",
                    "短内容保持单行输出；长内容如果适合分点，就用换行分点或编号。",
                    "不要凭空补充事实，不要解释，只返回最终可直接粘贴的文本。"
                )
                .into(),
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

fn build_user_prompt(transcript: &AsrOutput, pinyin_hint: &str) -> String {
    let segments = if transcript.segments.is_empty() {
        "无分段信息".to_string()
    } else {
        transcript
            .segments
            .iter()
            .map(|segment| format!("[{}-{}] {}", segment.start_ms, segment.end_ms, segment.text))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        concat!(
            "请根据以下信息输出最终可直接粘贴的文本。\n\n",
            "要求：\n",
            "1. 保留原意，但修复口语卡顿、重复、错词和断句问题。\n",
            "2. 内容较短时保持一行。\n",
            "3. 内容较长且天然包含多个点时，整理成分点或编号。\n",
            "4. 只输出最终文本，不要解释。\n\n",
            "原始转写：\n{}\n\n",
            "拼音提示：\n{}\n\n",
            "分段：\n{}\n"
        ),
        transcript.text, pinyin_hint, segments
    )
}
