use anyhow::{anyhow, Context, Result};
use std::cell::RefCell;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

use crate::{
    audio::ActiveRecording,
    bootstrap::{self, BootstrapResult},
    config::{self, AppConfig},
    events::{DeliveryResultEvent, RecordingState},
    hotkey,
    pipeline::{self, PipelineInput},
    state::{emit_delivery, emit_error, emit_progress, RuntimeState},
};

const MAIN_WINDOW_LABEL: &str = "main";
const OVERLAY_WINDOW_LABEL: &str = "overlay";
const OVERLAY_WIDTH: f64 = 360.0;
const OVERLAY_HEIGHT: f64 = 132.0;
const OVERLAY_BOTTOM_MARGIN: f64 = 92.0;

thread_local! {
    static ACTIVE_RECORDING: RefCell<Option<ActiveRecording>> = const { RefCell::new(None) };
}

#[tauri::command]
async fn bootstrap_resources(state: State<'_, RuntimeState>) -> Result<BootstrapResult, String> {
    let result = bootstrap::ensure_resources()
        .await
        .map_err(|error| error.to_string())?;

    let mut config = state.config.lock();
    config.whisper_sidecar_path = result.whisper_sidecar_path.clone();
    config.whisper_model_path = result.whisper_model_path.clone();
    let path = config::config_path().map_err(|error| error.to_string())?;
    config.save(&path).map_err(|error| error.to_string())?;

    Ok(result)
}

#[tauri::command]
fn get_settings(state: State<'_, RuntimeState>) -> AppConfig {
    state.config.lock().clone()
}

#[tauri::command]
fn save_settings(state: State<'_, RuntimeState>, settings: AppConfig) -> Result<AppConfig, String> {
    let path = config::config_path().map_err(|error| error.to_string())?;
    settings.save(&path).map_err(|error| error.to_string())?;
    *state.config.lock() = settings.clone();
    Ok(settings)
}

#[tauri::command]
fn start_recording(app: AppHandle, state: State<'_, RuntimeState>) -> Result<(), String> {
    start_recording_inner(&app, &state).map_err(|error| error.to_string())
}

#[tauri::command]
fn stop_recording(app: AppHandle, state: State<'_, RuntimeState>) -> Result<(), String> {
    stop_recording_inner(&app, &state).map_err(|error| error.to_string())
}

#[tauri::command]
fn retry_last_pipeline(app: AppHandle, state: State<'_, RuntimeState>) -> Result<(), String> {
    let input = state
        .last_input
        .lock()
        .clone()
        .map(|wav_path| PipelineInput { wav_path })
        .ok_or_else(|| "no previous recording available".to_string())?;

    spawn_pipeline(app, state.inner().clone(), input);
    Ok(())
}

#[tauri::command]
fn open_logs_folder(app: AppHandle) -> Result<(), String> {
    let dir = config::app_data_dir().map_err(|error| error.to_string())?;
    app.opener()
        .open_path(dir.display().to_string(), None::<String>)
        .map_err(|error| error.to_string())
}

fn start_recording_inner(app: &AppHandle, state: &State<'_, RuntimeState>) -> Result<()> {
    let config = state.config.lock().clone();
    let mut started = false;

    ACTIVE_RECORDING.with(|slot| -> Result<()> {
        let mut slot = slot.borrow_mut();
        if slot.is_some() {
            return Ok(());
        }

        let recording = ActiveRecording::start(config.microphone_device_id.as_deref())?;
        *slot = Some(recording);
        started = true;
        Ok(())
    })?;

    if started {
        hotkey::update_recording_flag(true);
        emit_progress(app, RecordingState::Recording, "录音中，按右 Alt 或点击停止结束。");
    }

    Ok(())
}

fn stop_recording_inner(app: &AppHandle, state: &State<'_, RuntimeState>) -> Result<()> {
    let recording = ACTIVE_RECORDING.with(|slot| slot.borrow_mut().take());
    let recording = recording.ok_or_else(|| anyhow!("recording is not active"))?;
    hotkey::update_recording_flag(false);

    let wav_path = recording.stop()?;
    *state.last_input.lock() = Some(wav_path.clone());
    spawn_pipeline(app.clone(), state.inner().clone(), PipelineInput { wav_path });
    Ok(())
}

pub fn toggle_recording_from_hotkey(app: &AppHandle) {
    if let Some(state) = app.try_state::<RuntimeState>() {
        let is_recording = ACTIVE_RECORDING.with(|slot| slot.borrow().is_some());
        let result = if is_recording {
            stop_recording_inner(app, &state)
        } else {
            start_recording_inner(app, &state)
        };

        if let Err(error) = result {
            emit_error(app, RecordingState::Error, error.to_string());
        }
    }
}

pub fn apply_recording_target(app: &AppHandle, target_recording: bool) {
    if let Some(state) = app.try_state::<RuntimeState>() {
        let is_recording = ACTIVE_RECORDING.with(|slot| slot.borrow().is_some());
        let result = match (target_recording, is_recording) {
            (true, false) => start_recording_inner(app, &state),
            (false, true) => stop_recording_inner(app, &state),
            _ => Ok(()),
        };

        if let Err(error) = result {
            emit_error(app, RecordingState::Error, error.to_string());
        }
    }
}

fn spawn_pipeline(app: AppHandle, state: RuntimeState, input: PipelineInput) {
    tauri::async_runtime::spawn(async move {
        emit_progress(&app, RecordingState::LocalTranscribing, "本地模型正在转写语音。");

        let config = state.config.lock().clone();
        let result = async {
            let transcript = pipeline::transcribe_local(&input, &config).await?;
            emit_progress(&app, RecordingState::LlmRepairing, "正在用 DeepSeek 修正拼音和语义。");
            let (_pinyin_hint, final_text) = pipeline::repair_text(&config, &transcript).await?;
            emit_progress(&app, RecordingState::Delivering, "正在投递结果到当前光标或剪贴板。");
            let delivery_mode = pipeline::deliver_text(&config, &final_text)?;
            pipeline::cleanup_temp_file(&input.wav_path);
            Ok::<_, anyhow::Error>((delivery_mode, final_text))
        }
        .await;

        match result {
            Ok((delivery_mode, final_text)) => {
                *state.last_output.lock() = Some(final_text.clone());
                emit_delivery(
                    &app,
                    DeliveryResultEvent {
                        mode: delivery_mode,
                        text: final_text.clone(),
                    },
                );

                if matches!(delivery_mode, crate::events::DeliveryMode::ClipboardOnly) {
                    let _ = app
                        .notification()
                        .builder()
                        .title("Typemore")
                        .body("转写结果已复制，可直接粘贴")
                        .show();
                }

                emit_progress(&app, RecordingState::Completed, "转写完成。");
            }
            Err(error) => {
                pipeline::cleanup_temp_file(&input.wav_path);
                hotkey::update_recording_flag(false);
                emit_error(&app, RecordingState::Error, error.to_string());
            }
        }
    });
}

fn setup_tray(app: &AppHandle) -> Result<()> {
    let show = MenuItem::with_id(app, "show", "打开设置", true, None::<&str>)?;
    let start = MenuItem::with_id(app, "start", "开始录音", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", "停止录音", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &start, &stop, &quit])?;

    let default_icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| anyhow!("default app icon not found"))?;

    TrayIconBuilder::with_id("main-tray")
        .icon(default_icon)
        .tooltip("Typemore")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                let _ = reveal_main_window(app);
            }
            "start" => {
                if let Some(state) = app.try_state::<RuntimeState>() {
                    let _ = start_recording_inner(app, &state);
                }
            }
            "stop" => {
                if let Some(state) = app.try_state::<RuntimeState>() {
                    let _ = stop_recording_inner(app, &state);
                }
            }
            "quit" => std::process::exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = reveal_main_window(tray.app_handle());
            }
        })
        .build(app)
        .context("failed to create tray icon")?;

    Ok(())
}

fn ensure_main_window(app: &AppHandle) -> Result<()> {
    if app.get_webview_window(MAIN_WINDOW_LABEL).is_some() {
        return Ok(());
    }

    WebviewWindowBuilder::new(app, MAIN_WINDOW_LABEL, WebviewUrl::App("index.html".into()))
        .title("Typemore")
        .inner_size(1120.0, 780.0)
        .resizable(true)
        .visible(true)
        .build()
        .context("failed to rebuild main window")?;

    Ok(())
}

fn reveal_main_window(app: &AppHandle) -> Result<()> {
    ensure_main_window(app)?;
    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| anyhow!("main window not found"))?;

    if window.is_minimized().unwrap_or(false) {
        let _ = window.unminimize();
    }

    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

fn setup_overlay_window(app: &AppHandle) -> Result<()> {
    if app.get_webview_window(OVERLAY_WINDOW_LABEL).is_some() {
        return Ok(());
    }

    let overlay = WebviewWindowBuilder::new(
        app,
        OVERLAY_WINDOW_LABEL,
        WebviewUrl::App("index.html?overlay=1".into()),
    )
    .title("Typemore Overlay")
    .transparent(true)
    .decorations(false)
    .shadow(false)
    .resizable(false)
    .skip_taskbar(true)
    .always_on_top(true)
    .visible(false)
    .focused(false)
    .inner_size(OVERLAY_WIDTH, OVERLAY_HEIGHT)
    .build()
    .context("failed to build overlay window")?;

    let _ = overlay.set_ignore_cursor_events(true);
    position_overlay_window(app)?;
    Ok(())
}

fn position_overlay_window(app: &AppHandle) -> Result<()> {
    let Some(overlay) = app.get_webview_window(OVERLAY_WINDOW_LABEL) else {
        return Ok(());
    };

    let monitor = app
        .primary_monitor()
        .context("failed to query primary monitor")?
        .ok_or_else(|| anyhow!("primary monitor not found"))?;

    let monitor_pos = monitor.position();
    let monitor_size = monitor.size();
    let x = monitor_pos.x as f64 + (monitor_size.width as f64 - OVERLAY_WIDTH) / 2.0;
    let y = monitor_pos.y as f64 + monitor_size.height as f64 - OVERLAY_HEIGHT - OVERLAY_BOTTOM_MARGIN;

    overlay
        .set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32))
        .context("failed to position overlay window")?;
    overlay
        .set_size(PhysicalSize::new(OVERLAY_WIDTH, OVERLAY_HEIGHT))
        .context("failed to size overlay window")?;

    Ok(())
}

pub fn sync_overlay_window(app: &AppHandle, state: RecordingState) {
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) else {
        return;
    };

    match state {
        RecordingState::Recording
        | RecordingState::LocalTranscribing
        | RecordingState::LlmRepairing
        | RecordingState::Delivering => {
            let _ = position_overlay_window(app);
            let _ = window.show();
        }
        RecordingState::Idle | RecordingState::Completed | RecordingState::Error => {
            let _ = window.hide();
        }
    }
}

pub fn run() {
    let config_path = config::config_path().expect("failed to resolve config path");
    let initial_config = AppConfig::load(&config_path).unwrap_or_default();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .on_window_event(|window, event| {
            if window.label() == MAIN_WINDOW_LABEL {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(move |app| {
            app.manage(RuntimeState::new(initial_config.clone()));
            setup_tray(app.handle())?;
            setup_overlay_window(app.handle())?;
            hotkey::install_alt_hook(app.handle().clone())?;
            emit_progress(app.handle(), RecordingState::Idle, "按右 Alt 开始录音，再按一次结束录音。");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            bootstrap_resources,
            start_recording,
            stop_recording,
            retry_last_pipeline,
            open_logs_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
