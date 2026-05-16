use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex, OnceLock,
    },
    thread,
    time::Duration,
};

use anyhow::{Context, Result};
use tauri::{AppHandle, Manager};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_RMENU};

use crate::{app, state::RuntimeState};

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();
static ACTUAL_RECORDING: AtomicBool = AtomicBool::new(false);
static PENDING_TARGET: OnceLock<Mutex<Option<bool>>> = OnceLock::new();
static PENDING_GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn install_alt_hook(app_handle: AppHandle) -> Result<()> {
    APP_HANDLE.set(app_handle).ok();
    let _ = PENDING_TARGET.set(Mutex::new(None));

    thread::Builder::new()
        .name("typemore-ralt-poller".into())
        .spawn(move || {
            let mut was_pressed = false;

            loop {
                let is_pressed = unsafe { (GetAsyncKeyState(i32::from(VK_RMENU.0)) as u16 & 0x8000) != 0 };

                if was_pressed && !is_pressed {
                    queue_toggle_request();
                }

                was_pressed = is_pressed;
                thread::sleep(Duration::from_millis(20));
            }
        })
        .context("failed to spawn right-alt polling thread")?;

    Ok(())
}

pub fn update_recording_flag(is_recording: bool) {
    ACTUAL_RECORDING.store(is_recording, Ordering::SeqCst);
}

fn queue_toggle_request() {
    let Some(app_handle) = APP_HANDLE.get().cloned() else {
        return;
    };
    let Some(state) = app_handle.try_state::<RuntimeState>() else {
        return;
    };

    let buffer_ms = state.config.lock().hotkey_buffer_ms.max(50);
    let next_generation = PENDING_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

    {
        let mut pending = PENDING_TARGET
            .get()
            .expect("pending target not initialized")
            .lock()
            .expect("pending target poisoned");
        let base_target = pending.unwrap_or_else(|| ACTUAL_RECORDING.load(Ordering::SeqCst));
        *pending = Some(!base_target);
    }

    thread::spawn(move || {
        thread::sleep(Duration::from_millis(buffer_ms));

        if PENDING_GENERATION.load(Ordering::SeqCst) != next_generation {
            return;
        }

        let target = {
            let mut pending = PENDING_TARGET
                .get()
                .expect("pending target not initialized")
                .lock()
                .expect("pending target poisoned");
            pending.take()
        };

        let Some(target) = target else {
            return;
        };

        let app_for_closure = app_handle.clone();
        let _ = app_handle.run_on_main_thread(move || {
            app::apply_recording_target(&app_for_closure, target);
        });
    });
}
