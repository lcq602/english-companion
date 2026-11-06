//! 输入监听编排：剪贴板监听（全平台）+ Windows UI Automation 焦点输入框监听（仅 Windows）。
//!
//! 安全约束（对应需求第 7 节）：
//! - 密码框一律跳过（UIA IsPassword / 前端不再二次处理）。
//! - 应用黑名单：命中的进程不监听。
//! - 一键暂停：paused 为 true 时两个监听都不再发出任何文本。
//! - 本应用自己进程不监听，避免自激。
//! - 只发出「当前输入框/剪贴板文本」这一事件，不落盘、不写日志，不记录原始键盘输入。

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tauri::{AppHandle, Emitter};

#[derive(Clone)]
pub struct MonitorState {
    pub paused: Arc<AtomicBool>,
    pub clipboard_enabled: Arc<AtomicBool>,
    pub uia_enabled: Arc<AtomicBool>,
    pub blacklist: Arc<RwLock<Vec<String>>>,
    /// 程序自己写剪贴板（复制译文）后短暂抑制，避免回环触发翻译。
    pub suppress_clipboard_until: Arc<std::sync::Mutex<std::time::Instant>>,
}

impl Default for MonitorState {
    fn default() -> Self {
        Self {
            paused: Arc::new(AtomicBool::new(false)),
            clipboard_enabled: Arc::new(AtomicBool::new(true)),
            uia_enabled: Arc::new(AtomicBool::new(true)),
            blacklist: Arc::new(RwLock::new(default_blacklist())),
            suppress_clipboard_until: Arc::new(std::sync::Mutex::new(
                std::time::Instant::now(),
            )),
        }
    }
}

pub fn default_blacklist() -> Vec<String> {
    // 常见密码管理器 / 银行类应用默认不监听
    vec![
        "1password".to_string(),
        "keepass".to_string(),
        "bitwarden".to_string(),
        "lastpass".to_string(),
    ]
}

impl MonitorState {
    pub fn is_blacklisted(&self, process_name: &str) -> bool {
        let lower = process_name.to_lowercase();
        self.blacklist
            .read()
            .map(|list| {
                list.iter().any(|item| {
                    let item = item.trim().to_lowercase();
                    !item.is_empty() && lower.contains(&item)
                })
            })
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TextEvent {
    pub text: String,
    pub source: String, // "uia" | "clipboard"
    pub app: String,
}

pub fn start_monitors(app: AppHandle, state: MonitorState) {
    start_clipboard_monitor(app.clone(), state.clone());
    start_uia_monitor(app, state);
}

// ---------- 剪贴板监听 ----------

fn start_clipboard_monitor(app: AppHandle, state: MonitorState) {
    std::thread::spawn(move || {
        let mut last_hash = String::new();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(650));
            if state.paused.load(Ordering::Relaxed) || !state.clipboard_enabled.load(Ordering::Relaxed) {
                continue;
            }
            if let Ok(until) = state.suppress_clipboard_until.lock() {
                if std::time::Instant::now() < *until {
                    continue;
                }
            }
            let text = match arboard::Clipboard::new() {
                Ok(mut cb) => cb.get_text().unwrap_or_default(),
                Err(_) => continue,
            };
            let trimmed = text.trim().to_string();
            if trimmed.chars().count() < 2 || trimmed.chars().count() > 2000 {
                continue;
            }
            let hash = crate::db::cache_key(&trimmed, "", "");
            if hash == last_hash {
                continue;
            }
            last_hash = hash;
            let _ = app.emit(
                "companion-text",
                TextEvent { text: trimmed, source: "clipboard".to_string(), app: String::new() },
            );
        }
    });
}

// ---------- Windows UI Automation 监听 ----------

#[cfg(not(windows))]
fn start_uia_monitor(_app: AppHandle, _state: MonitorState) {
    // 非 Windows 平台无 UIA，静默跳过（应用主体功能不受影响）。
}

#[cfg(windows)]
fn start_uia_monitor(app: AppHandle, state: MonitorState) {
    std::thread::spawn(move || {
        let mut last_text = String::new();
        let own_pid = std::process::id();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if state.paused.load(Ordering::Relaxed) || !state.uia_enabled.load(Ordering::Relaxed) {
                continue;
            }
            let Some((text, process_name, pid)) = focused_input_text() else {
                continue;
            };
            if pid == own_pid {
                continue; // 不监听本应用自己的输入框
            }
            if state.is_blacklisted(&process_name) {
                continue;
            }
            let trimmed = text.trim().to_string();
            if trimmed.chars().count() < 1 || trimmed.chars().count() > 2000 {
                continue;
            }
            if trimmed == last_text {
                continue; // 文本去重：没变化不重复发出
            }
            last_text = trimmed.clone();
            let _ = app.emit(
                "companion-text",
                TextEvent { text: trimmed, source: "uia".to_string(), app: process_name },
            );
        }
    });
}

/// 读取当前焦点输入框的文本。返回 (文本, 进程名, 进程 id)。
/// 密码框直接返回 None，绝不读取其内容。
#[cfg(windows)]
fn focused_input_text() -> Option<(String, String, u32)> {
    use uiautomation::patterns::{UITextPattern, UIValuePattern};
    use uiautomation::UIAutomation;

    let automation = UIAutomation::new().ok()?;
    let element = automation.get_focused_element().ok()?;
    if element.is_password().unwrap_or(false) {
        return None; // 安全要求：不捕获密码框内容
    }
    let pid = element.get_process_id().unwrap_or(0);
    let process_name = process_name_of(pid);

    // 优先 ValuePattern（普通输入框），其次 TextPattern（文档型编辑器）
    if let Ok(value_pattern) = element.get_pattern::<UIValuePattern>() {
        if let Ok(value) = value_pattern.get_value() {
            if !value.trim().is_empty() {
                return Some((value, process_name, pid));
            }
        }
    }
    if let Ok(text_pattern) = element.get_pattern::<UITextPattern>() {
        if let Ok(range) = text_pattern.get_document_range() {
            if let Ok(text) = range.get_text(-1) {
                if !text.trim().is_empty() {
                    return Some((text, process_name, pid));
                }
            }
        }
    }
    None
}

#[cfg(windows)]
fn process_name_of(pid: u32) -> String {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::ProcessStatus::GetModuleBaseNameW;
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};

    if pid == 0 {
        return String::new();
    }
    unsafe {
        let handle = match OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) {
            Ok(h) => h,
            Err(_) => return String::new(),
        };
        let mut buffer = [0u16; 260];
        let len = GetModuleBaseNameW(handle, None, &mut buffer);
        let _ = CloseHandle(handle);
        if len == 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buffer[..len as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blacklist_matches_substring_case_insensitive() {
        let state = MonitorState::default();
        assert!(state.is_blacklisted("1Password.exe"));
        assert!(state.is_blacklisted("KeePassXC.exe"));
        assert!(!state.is_blacklisted("notepad.exe"));
    }
}
