use std::{fs, path::Path};

use anyhow::{anyhow, Context, Result};
use tokio::process::Command;
use uuid::Uuid;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone)]
pub struct AsrSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct AsrOutput {
    pub text: String,
    pub segments: Vec<AsrSegment>,
}

pub async fn transcribe(
    sidecar_path: &str,
    model_path: &str,
    input_wav: &Path,
    language: &str,
) -> Result<AsrOutput> {
    if sidecar_path.trim().is_empty() {
        return Err(anyhow!("whisper sidecar path is empty"));
    }
    if model_path.trim().is_empty() {
        return Err(anyhow!("whisper model path is empty"));
    }

    let output_prefix = input_wav
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("transcript-{}", Uuid::new_v4()));
    let output_prefix_str = output_prefix.to_string_lossy().to_string();

    let mut command = Command::new(sidecar_path);
    command
        .arg("-m")
        .arg(model_path)
        .arg("-f")
        .arg(input_wav)
        .arg("-l")
        .arg(language)
        .arg("-nt")
        .arg("-of")
        .arg(&output_prefix_str)
        .arg("-otxt");

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command
        .output()
        .await
        .with_context(|| format!("failed to execute whisper sidecar {sidecar_path}"))?;

    if !output.status.success() {
        return Err(anyhow!(
            "whisper sidecar exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let text_path = output_prefix.with_extension("txt");
    let text = fs::read_to_string(&text_path)
        .with_context(|| format!("failed to read whisper output {}", text_path.display()))?
        .trim()
        .to_string();
    let _ = fs::remove_file(&text_path);

    if text.is_empty() {
        return Err(anyhow!("whisper produced empty transcript"));
    }

    Ok(AsrOutput {
        text,
        segments: Vec::new(),
    })
}
