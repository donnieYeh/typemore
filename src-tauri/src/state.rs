use std::{path::PathBuf, sync::Arc};

use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};

use crate::{
    app,
    config::AppConfig,
    events::{DeliveryResultEvent, ErrorEvent, ProgressEvent, RecordingState},
};

#[derive(Clone)]
pub struct RuntimeState {
    pub config: Arc<Mutex<AppConfig>>,
    pub last_input: Arc<Mutex<Option<PathBuf>>>,
    pub last_output: Arc<Mutex<Option<String>>>,
}

impl RuntimeState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: Arc::new(Mutex::new(config)),
            last_input: Arc::new(Mutex::new(None)),
            last_output: Arc::new(Mutex::new(None)),
        }
    }
}

pub fn emit_progress(app_handle: &AppHandle, state: RecordingState, message: impl Into<String>) {
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
