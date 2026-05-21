use std::{
    collections::VecDeque,
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

static ENTER_TEXT_CAPTURE: OnceLock<Mutex<EnterTextCapture>> = OnceLock::new();

pub struct EnterTextCapture {
    pub last_submit_text: String,
    pub history: VecDeque<UserCorrection>,
    pub pending_enter: bool,
}

#[derive(Debug, Clone)]
pub struct UserCorrection {
    pub original: String,
    pub final_text: String,
    pub timestamp: std::time::Instant,
}

pub fn install_alt_hook(app_handle: AppHandle) -> Result<()> {
    APP_HANDLE.set(app_handle).ok();
    let _ = PENDING_TARGET.set(Mutex::new(None));
    let _ = ENTER_TEXT_CAPTURE.set(Mutex::new(EnterTextCapture {
        last_submit_text: String::new(),
        history: VecDeque::with_capacity(20),
        pending_enter: false,
    }));

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

    thread::Builder::new()
        .name("typemore-enter-hook".into())
        .spawn(move || {
            let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(enter_hook_proc), None, 0) };
            if hook.is_err() {
                return;
            }

            let mut message = MSG::default();
            while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
                // Driven by message loop
            }
        })
        .context("failed to spawn enter hook thread")?;

    Ok(())
}

pub fn record_pasted_text(text: &str) {
    if let Some(capture) = ENTER_TEXT_CAPTURE.get() {
        let mut c = capture.lock().unwrap();
        c.last_submit_text = text.to_string();
        c.pending_enter = false;
    }
}

pub fn get_corrections() -> Vec<UserCorrection> {
    ENTER_TEXT_CAPTURE
        .get()
        .map(|c| c.lock().unwrap().history.iter().cloned().collect())
        .unwrap_or_default()
}

pub fn update_recording_flag(is_recording: bool) {
    ACTUAL_RECORDING.store(is_recording, Ordering::SeqCst);
}

unsafe fn get_focused_edit_text() -> Option<String> {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

    let automation = uiautomation::UIAutomation::new().ok()?;
    let focused = automation.get_focused_element().ok()?;

    // Check if it's an edit control
    let is_edit = focused.get_control_type()
        .map(|ct| {
            let kind = format!("{ct:?}");
            kind.contains("Edit") || kind.contains("Document") || kind.contains("Text")
        })
        .unwrap_or(false);

    if !is_edit {
        return None;
    }

    // Get text pattern from focused element
    let text_pattern = match focused.get_pattern::<uiautomation::patterns::UITextPattern>() {
        Ok(tp) => tp,
        Err(_) => return None,
    };

    // Get the full document range text
    text_pattern.get_document_range().ok()
        .and_then(|range| range.get_text(-1).ok())
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

unsafe extern "system" fn enter_hook_proc(code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if code >= 0 {
        let event = w_param.0 as u32;
        let keyboard_event = unsafe { &*(l_param.0 as *const KBDLLHOOKSTRUCT) };

        if event == WM_KEYDOWN && keyboard_event.vkCode == 0x0D {
            // Capture text at the moment of Enter keydown
            if let Some(text) = get_focused_edit_text() {
                if !text.is_empty() {
                    if let Some(capture) = ENTER_TEXT_CAPTURE.get() {
                        let mut c = capture.lock().unwrap();
                        // Only update if not already tracking a pending enter
                        if !c.pending_enter {
                            c.last_submit_text = text;
                            c.pending_enter = true;
                        }
                    }
                }
            }
        }

        if event == WM_KEYUP && keyboard_event.vkCode == 0x0D {
            if let Some(capture) = ENTER_TEXT_CAPTURE.get() {
                let mut c = capture.lock().unwrap();
                if c.pending_enter && !c.last_submit_text.is_empty() {
                    let original = c.last_submit_text.clone();
                    if c.history.len() >= 20 {
                        c.history.pop_back();
                    }
                    c.history.push_front(UserCorrection {
                        original,
                        final_text: String::new(),
                        timestamp: std::time::Instant::now(),
                    });
                    c.last_submit_text.clear();
                    c.pending_enter = false;
                }
            }
        }
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
