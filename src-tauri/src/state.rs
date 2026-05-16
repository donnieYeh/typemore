use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use parking_lot::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::OnceCell;

use crate::{
    asr::AsrWorker,
    app,
    config::AppConfig,
    events::{DeliveryResultEvent, ErrorEvent, ProgressEvent, RecordingState},
};

#[derive(Clone)]
pub struct RuntimeState {
    pub config: Arc<Mutex<AppConfig>>,
    pub last_input: Arc<Mutex<Option<PathBuf>>>,
    pub last_output: Arc<Mutex<Option<String>>>,
    pub pipeline_busy: Arc<AtomicBool>,
    pub current_state: Arc<Mutex<RecordingState>>,
    pub asr_worker: Arc<OnceCell<AsrWorker>>,
}

impl RuntimeState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: Arc::new(Mutex::new(config)),
            last_input: Arc::new(Mutex::new(None)),
            last_output: Arc::new(Mutex::new(None)),
            pipeline_busy: Arc::new(AtomicBool::new(false)),
            current_state: Arc::new(Mutex::new(RecordingState::Idle)),
            asr_worker: Arc::new(OnceCell::new()),
        }
    }

    pub fn is_pipeline_busy(&self) -> bool {
        self.pipeline_busy.load(Ordering::SeqCst)
    }

    pub fn try_begin_pipeline(&self) -> bool {
        self.pipeline_busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub fn finish_pipeline(&self) {
        self.pipeline_busy.store(false, Ordering::SeqCst);
    }

    pub async fn asr_worker(&self) -> AsrWorker {
        self.asr_worker
            .get_or_init(|| async { AsrWorker::new() })
            .await
            .clone()
    }
}

pub fn emit_progress(app_handle: &AppHandle, state: RecordingState, message: impl Into<String>) {
    if let Some(runtime) = app_handle.try_state::<RuntimeState>() {
        *runtime.current_state.lock() = state;
    }
    app::sync_overlay_window(app_handle, state);
    let _ = app_handle.emit(
        "pipeline-progress",
        ProgressEvent {
            state,
            message: message.into(),
        },
    );
}

pub fn emit_error(app_handle: &AppHandle, state: RecordingState, error: impl Into<String>) {
    if let Some(runtime) = app_handle.try_state::<RuntimeState>() {
        *runtime.current_state.lock() = state;
    }
    app::sync_overlay_window(app_handle, state);
    let _ = app_handle.emit(
        "pipeline-error",
        ErrorEvent {
            state,
            error: error.into(),
        },
    );
}

pub fn emit_delivery(app_handle: &AppHandle, event: DeliveryResultEvent) {
    let _ = app_handle.emit("delivery-result", event);
}
