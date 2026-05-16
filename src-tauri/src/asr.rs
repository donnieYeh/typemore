use std::{fs, path::Path};

use anyhow::{anyhow, Context, Result};
use tokio::{
    process::Command,
    sync::{mpsc, oneshot},
};
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

#[derive(Clone)]
pub struct AsrWorker {
    sender: mpsc::Sender<AsrJob>,
}

struct AsrJob {
    request: TranscribeRequest,
    respond_to: oneshot::Sender<Result<AsrOutput>>,
}

#[derive(Debug)]
struct TranscribeRequest {
    sidecar_path: String,
    model_path: String,
    input_wav: std::path::PathBuf,
    language: String,
}

impl AsrWorker {
    pub fn new() -> Self {
        let (sender, mut receiver) = mpsc::channel::<AsrJob>(8);
        tauri::async_runtime::spawn(async move {
            while let Some(job) = receiver.recv().await {
                let result = transcribe_cli(
                    &job.request.sidecar_path,
                    &job.request.model_path,
                    &job.request.input_wav,
                    &job.request.language,
                )
                .await;
                let _ = job.respond_to.send(result);
            }
        });

        Self { sender }
    }

    pub async fn transcribe(
        &self,
        sidecar_path: &str,
        model_path: &str,
        input_wav: &Path,
        language: &str,
    ) -> Result<AsrOutput> {
        let (respond_to, response) = oneshot::channel();
        self.sender
            .send(AsrJob {
                request: TranscribeRequest {
                    sidecar_path: sidecar_path.to_string(),
                    model_path: model_path.to_string(),
                    input_wav: input_wav.to_path_buf(),
                    language: language.to_string(),
                },
                respond_to,
            })
            .await
            .context("asr worker is not available")?;

        response.await.context("asr worker stopped unexpectedly")?
    }
}

pub async fn transcribe(
    sidecar_path: &str,
    model_path: &str,
    input_wav: &Path,
    language: &str,
) -> Result<AsrOutput> {
    AsrWorker::new()
        .transcribe(sidecar_path, model_path, input_wav, language)
        .await
}

async fn transcribe_cli(
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
