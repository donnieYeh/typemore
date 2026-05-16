use anyhow::{Context, Result};
use arboard::Clipboard;
use enigo::{Enigo, Key, KeyboardControllable};
#[cfg(target_os = "windows")]
use std::{thread, time::Duration};
#[cfg(target_os = "windows")]
use windows::Win32::{
    Foundation::HWND,
    System::Threading::GetCurrentProcessId,
    UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
};

use crate::events::DeliveryMode;

pub fn deliver_text(text: &str, auto_paste: bool) -> Result<DeliveryMode> {
    let mut clipboard = Clipboard::new().context("failed to access clipboard")?;
    clipboard
        .set_text(text.to_string())
        .context("failed to write text to clipboard")?;

    if auto_paste && can_paste_into_foreground_app() {
        thread::sleep(Duration::from_millis(60));
        if paste_clipboard().is_ok() {
            return Ok(DeliveryMode::Pasted);
        }
    }

    if auto_paste && is_editable_focus() && paste_clipboard().is_ok() {
        return Ok(DeliveryMode::Pasted);
    }

    Ok(DeliveryMode::ClipboardOnly)
}

fn paste_clipboard() -> Result<()> {
    let mut enigo = Enigo::new();
    enigo.key_down(Key::Control);
    enigo.key_click(Key::Layout('v'));
    enigo.key_up(Key::Control);
    Ok(())
}

#[cfg(target_os = "windows")]
fn is_editable_focus() -> bool {
    use uiautomation::UIAutomation;

    let automation = match UIAutomation::new() {
        Ok(automation) => automation,
        Err(_) => return false,
    };
    let focused = match automation.get_focused_element() {
        Ok(element) => element,
        Err(_) => return false,
    };

    if let Ok(control_type) = focused.get_control_type() {
        let kind = format!("{control_type:?}");
        return kind.contains("Edit") || kind.contains("Document");
    }

    false
}

#[cfg(target_os = "windows")]
fn can_paste_into_foreground_app() -> bool {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground == HWND::default() {
        return false;
    }

    let mut process_id = 0u32;
    unsafe {
        let _ = GetWindowThreadProcessId(foreground, Some(&mut process_id));
    }

    process_id != 0 && process_id != unsafe { GetCurrentProcessId() }
}

#[cfg(not(target_os = "windows"))]
fn is_editable_focus() -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
fn can_paste_into_foreground_app() -> bool {
    false
}
