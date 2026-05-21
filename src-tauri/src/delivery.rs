use std::{collections::VecDeque, sync::Mutex};
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

static CLIPBOARD_HISTORY: once_cell::sync::Lazy<Mutex<VecDeque<ClipboardEntry>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(VecDeque::with_capacity(10)));

struct ClipboardEntry {
    text: String,
    timestamp: std::time::Instant,
    was_pasted: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CursorContext {
    pub before: String,
    pub after: String,
    pub line_start: String,
    pub indentation: String,
    pub ends_with_punctuation: bool,
    pub is_in_list: bool,
    pub list_marker: String,
    pub follows_numbered_list: bool,
    pub current_number: Option<u32>,
}

#[cfg(target_os = "windows")]
pub fn get_cursor_context() -> Option<CursorContext> {
    None
}

#[cfg(not(target_os = "windows"))]
pub fn get_cursor_context() -> Option<CursorContext> {
    None
}

fn detect_list_context(before: &str) -> (bool, String, bool, Option<u32>) {
    let lines: Vec<&str> = before.split('\n').collect();
    if lines.len() < 2 {
        return (false, String::new(), false, None);
    }

    let last_line = lines[lines.len() - 1];
    let prev_line = lines[lines.len() - 2];

    if last_line.trim().is_empty() || last_line.trim().chars().all(|c| c.is_whitespace()) {
        if let Some(list_info) = check_list_pattern(prev_line) {
            return (true, list_info.marker, list_info.numbered, list_info.number);
        }
    }

    if let Some(list_info) = check_list_pattern(last_line) {
        return (true, list_info.marker, list_info.numbered, list_info.number);
    }

    (false, String::new(), false, None)
}

struct ListInfo {
    marker: String,
    numbered: bool,
    number: Option<u32>,
}

fn check_list_pattern(line: &str) -> Option<ListInfo> {
    let trimmed = line.trim();

    if trimmed.starts_with("- ") || trimmed.starts_with("• ") || trimmed.starts_with("· ") {
        return Some(ListInfo {
            marker: "- ".to_string(),
            numbered: false,
            number: None,
        });
    }

    if trimmed.starts_with("·") || trimmed.starts_with("•") {
        return Some(ListInfo {
            marker: "• ".to_string(),
            numbered: false,
            number: None,
        });
    }

    if let Some(rest) = trimmed.strip_prefix('*') {
        if rest.starts_with(' ') || rest.is_empty() {
            return Some(ListInfo {
                marker: "* ".to_string(),
                numbered: false,
                number: None,
            });
        }
    }

    let numbered_patterns = [
        (r"^(\d+)[.．、]", 1),
        (r"^([一二三四五六七八九十百千]+)[.．、]", 1),
        (r"^([ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz]+)[.．、]", 1),
    ];

    for (pattern, _group) in numbered_patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if let Some(caps) = re.captures(trimmed) {
                if let Some(num_match) = caps.get(1) {
                    let num_str = num_match.as_str();
                    let number = if num_str.chars().all(|c| c.is_ascii_digit()) {
                        num_str.parse().ok()
                    } else {
                        chinese_to_number(num_str)
                    };
                    let marker = format!("{}. ", num_str);
                    return Some(ListInfo {
                        marker,
                        numbered: true,
                        number,
                    });
                }
            }
        }
    }

    None
}

fn chinese_to_number(s: &str) -> Option<u32> {
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return None;
    }

    let mut result = 0u32;
    for ch in chars {
        let val = match ch {
            '一' | '壹' => 1,
            '二' | '贰' => 2,
            '三' | '叁' => 3,
            '四' | '肆' => 4,
            '五' | '伍' => 5,
            '六' | '陆' => 6,
            '七' | '柒' => 7,
            '八' | '捌' => 8,
            '九' | '玖' => 9,
            '十' | '拾' => 10,
            _ => return None,
        };
        result = result * 10 + val;
    }
    Some(result).filter(|&n| n > 0)
}

pub fn record_paste_attempt(text: &str) {
    let mut history = CLIPBOARD_HISTORY.lock().unwrap();
    history.push_front(ClipboardEntry {
        text: text.to_string(),
        timestamp: std::time::Instant::now(),
        was_pasted: false,
    });
    if history.len() > 10 {
        history.pop_back();
    }
}

pub fn mark_as_pasted(text: &str) {
    let mut history = CLIPBOARD_HISTORY.lock().unwrap();
    if let Some(entry) = history.iter_mut().find(|e| e.text == text && !e.was_pasted) {
        entry.was_pasted = true;
    }
}

pub fn detect_dissatisfaction() -> bool {
    let history = CLIPBOARD_HISTORY.lock().unwrap();
    if let Some(last) = history.front() {
        if last.was_pasted && last.timestamp.elapsed().as_secs() < 5 {
            return false;
        }
        for entry in history.iter().take(3) {
            if entry.was_pasted && entry.timestamp.elapsed().as_secs() < 15 {
                return true;
            }
        }
    }
    false
}

pub fn deliver_text(text: &str, auto_paste: bool) -> Result<DeliveryMode> {
    record_paste_attempt(text);

    let mut clipboard = Clipboard::new().context("failed to access clipboard")?;
    clipboard
        .set_text(text.to_string())
        .context("failed to write text to clipboard")?;

    if auto_paste && can_paste_into_foreground_app() {
        thread::sleep(Duration::from_millis(60));
        if paste_clipboard().is_ok() {
            mark_as_pasted(text);
            return Ok(DeliveryMode::Pasted);
        }
    }

    if auto_paste && is_editable_focus() && paste_clipboard().is_ok() {
        mark_as_pasted(text);
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
