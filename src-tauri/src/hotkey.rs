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
use windows::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    UI::{
        Input::KeyboardAndMouse::VK_RMENU,
        WindowsAndMessaging::{
            CallNextHookEx, GetMessageW, KBDLLHOOKSTRUCT, SetWindowsHookExW, MSG,
            WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
        },
    },
};

use crate::{app, state::RuntimeState};

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();
static ACTUAL_RECORDING: AtomicBool = AtomicBool::new(false);
static RALT_PRESSED: AtomicBool = AtomicBool::new(false);
static PENDING_TARGET: OnceLock<Mutex<Option<bool>>> = OnceLock::new();
static PENDING_GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn install_alt_hook(app_handle: AppHandle) -> Result<()> {
    APP_HANDLE.set(app_handle).ok();
    let _ = PENDING_TARGET.set(Mutex::new(None));

    thread::Builder::new()
        .name("typemore-ralt-hook".into())
        .spawn(move || {
            let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0) };
            let Ok(_hook) = hook else {
                return;
            };

            let mut message = MSG::default();
            while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
                // The hook is driven by this message loop. Tauri owns application dispatch.
            }
        })
        .context("failed to spawn right-alt hook thread")?;

    Ok(())
}

pub fn update_recording_flag(is_recording: bool) {
    ACTUAL_RECORDING.store(is_recording, Ordering::SeqCst);
}

unsafe extern "system" fn keyboard_hook_proc(code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if code < 0 {
        return unsafe { CallNextHookEx(None, code, w_param, l_param) };
    }

    let event = w_param.0 as u32;
    let keyboard_event = unsafe { &*(l_param.0 as *const KBDLLHOOKSTRUCT) };

    if keyboard_event.vkCode == u32::from(VK_RMENU.0) {
        match event {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                RALT_PRESSED.store(true, Ordering::SeqCst);
            }
            WM_KEYUP | WM_SYSKEYUP => {
                if RALT_PRESSED.swap(false, Ordering::SeqCst) {
                    queue_toggle_request();
                }
            }
            _ => {}
        }

        return LRESULT(1);
    }

    unsafe { CallNextHookEx(None, code, w_param, l_param) }
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
