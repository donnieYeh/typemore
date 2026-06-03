use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{asr::AsrOutput, config::{AppConfig, PromptStyle}, delivery::CursorContext, hotkey::UserCorrection};

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    stream: bool,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<serde_json::Value>,
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

pub async fn repair_text(
    config: &AppConfig,
    transcript: &AsrOutput,
    pinyin_hint: &str,
    cursor_context: &CursorContext,
    dissatisfaction: bool,
    corrections: &[UserCorrection],
) -> Result<String> {
    if config.deepseek_api_key.trim().is_empty() {
        return Err(anyhow!("deepseek api key is empty"));
    }

    let full_prompt = build_full_prompt(
        &config.prompt_style,
        transcript,
        pinyin_hint,
        cursor_context,
        dissatisfaction,
        corrections,
    );

    crate::app::record_prompt(full_prompt.clone());

    let client = Client::new();
    let messages = vec![
        Message {
            role: "system",
            content: full_prompt.split("\n\n===USER_PROMPT===\n").next().unwrap_or("").to_string(),
        },
        Message {
            role: "user",
            content: full_prompt.split("\n\n===USER_PROMPT===\n").nth(1).unwrap_or("").to_string(),
        },
    ];

    let request = ChatRequest {
        model: "deepseek-v4-flash",
        stream: false,
        messages,
        thinking: None,
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

pub fn build_full_prompt(
    style: &PromptStyle,
    transcript: &AsrOutput,
    pinyin_hint: &str,
    cursor_context: &CursorContext,
    dissatisfaction: bool,
    corrections: &[UserCorrection],
) -> String {
    let system = build_system_prompt(style, dissatisfaction, corrections);
    let user = build_user_prompt(transcript, pinyin_hint, cursor_context, corrections);
    format!("{}\n\n===USER_PROMPT===\n{}", system, user)
}

fn build_system_prompt(style: &PromptStyle, dissatisfaction: bool, corrections: &[UserCorrection]) -> String {
    let base = match style {
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
    };

    let mut result = base;

    if !corrections.is_empty() {
        result = format!(
            "{} 用户之前的修改记录供参考：{}",
            result,
            corrections
                .iter()
                .take(3)
                .map(|c| format!("原文「{}」→ 用户修改为「{}」", c.original, c.final_text))
                .collect::<Vec<_>>()
                .join("；")
        );
    }

    if dissatisfaction {
        result = format!(
            "{} 用户对之前的转写结果表示不满，请更加谨慎地处理，特别注意可能存在同音字错误。",
            result
        );
    }

    result
}

fn build_user_prompt(
    transcript: &AsrOutput,
    pinyin_hint: &str,
    cursor_context: &CursorContext,
    corrections: &[UserCorrection],
) -> String {
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

    let context_section = if !cursor_context.before.is_empty() || !cursor_context.after.is_empty() {
        let mut ctx = String::new();
        ctx.push_str("\n\n光标上下文：");
        if !cursor_context.before.is_empty() {
            ctx.push_str("\n光标前文：");
            ctx.push_str(&cursor_context.before);
        }
        if !cursor_context.after.is_empty() {
            ctx.push_str("\n光标后文：");
            ctx.push_str(&cursor_context.after);
        }
        if !cursor_context.indentation.is_empty() {
            ctx.push_str("\n当前缩进：");
            ctx.push_str(&cursor_context.indentation);
        }
        if cursor_context.is_in_list {
            ctx.push_str("\n当前在列表中，标记：");
            ctx.push_str(&cursor_context.list_marker);
            if cursor_context.follows_numbered_list {
                ctx.push_str("（编号列表，下一个序号：");
                ctx.push_str(&(cursor_context.current_number.unwrap_or(0) + 1).to_string());
                ctx.push_str("）");
            }
        }
        if !cursor_context.ends_with_punctuation && !cursor_context.before.is_empty() {
            ctx.push_str("\n注意：光标前文未以标点结尾，输出应自然衔接。");
        }
        ctx
    } else {
        String::new()
    };

    let correction_section = if !corrections.is_empty() {
        format!(
            "\n\n用户修改历史（学习这些模式，避免重复错误）：\n{}",
            corrections
                .iter()
                .take(5)
                .map(|c| format!("- 原文：{} → 终稿：{}", c.original, c.final_text))
                .collect::<Vec<_>>()
                .join("\n")
        )
    } else {
        String::new()
    };

    format!(
        "请输出最终润色后的文本，并统一使用简体中文。{}{}\n\n原始转写：\n{}\n\n拼音提示：\n{}\n\n分段信息：\n{}\n",
        context_section, correction_section, transcript.text, pinyin_hint, segments
    )
}