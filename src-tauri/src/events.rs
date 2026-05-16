use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
pub enum RecordingState {
    Idle,
    Recording,
    LocalTranscribing,
    LlmRepairing,
    Delivering,
    Completed,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    pub state: RecordingState,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorEvent {
    pub state: RecordingState,
    pub error: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum DeliveryMode {
    Pasted,
    ClipboardOnly,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeliveryResultEvent {
    pub mode: DeliveryMode,
    pub text: String,
}
