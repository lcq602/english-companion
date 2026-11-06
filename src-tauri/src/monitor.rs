//! 输入监听编排：剪贴板监听（全平台）+ Windows UI Automation 焦点输入框监听（仅 Windows）。
//!
//! 安全约束（对应需求第 7 节）：
//! - 密码框一律跳过（UIA IsPassword / 前端不再二次处理）。
//! - 应用黑名单：命中的进程不监听。
//! - 一键暂停：paused 为 true 时两个监听都不再发出任何文本。
//! - 本应用自己进程不监听，避免自激。
//! - 只发出「当前输入框/剪贴板文本」这一事件，不落盘、不写日志，不记录原始键盘输入。
//! - 回车触发模式下，输入框文本只暂存在内存中；监听到回车键按下沿才发出一次，
//!   按键只做状态轮询观察，不拦截、不修改用户在其它软件里的回车行为。

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tauri::{AppHandle, Emitter};

#[derive(Clone)]
pub struct MonitorState {
    pub paused: Arc<AtomicBool>,
    pub clipboard_enabled: Arc<AtomicBool>,
    pub uia_enabled: Arc<AtomicBool>,
    /// true=只有在当前输入框按下回车才翻译；false=输入停顿后自动翻译。
    pub enter_only: Arc<AtomicBool>,
    /// 回车模式下暂存的当前输入框文本（仅内存，不落盘）。
    pub latest_uia: Arc<RwLock<Option<StagedText>>>,
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
            enter_only: Arc::new(AtomicBool::new(true)),
            latest_uia: Arc::new(RwLock::new(None)),
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

#[derive(Debug, Clone)]
pub struct StagedText {
    pub event: TextEvent,
    pub pid: u32,
}

pub fn start_monitors(app: AppHandle, state: MonitorState) {
    start_clipboard_monitor(app.clone(), state.clone());
    start_uia_monitor(app.clone(), state.clone());
    start_key_trigger_monitor(app, state);
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
            if state.enter_only.load(Ordering::Relaxed) {
                continue; // 回车模式只翻译当前输入框；剪贴板可用 Ctrl+Shift+T 主动送译
            }
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
            let enter_only = state.enter_only.load(Ordering::Relaxed);
            std::thread::sleep(std::time::Duration::from_millis(if enter_only { 250 } else { 500 }));
            if state.paused.load(Ordering::Relaxed) || !state.uia_enabled.load(Ordering::Relaxed) {
                if enter_only {
                    clear_staged_uia(&state);
                }
                continue;
            }
            let Some(info) = focused_input_info() else {
                continue;
            };
            if info.pid == own_pid || info.is_password || state.is_blacklisted(&info.process_name) {
                if enter_only {
                    clear_staged_uia(&state);
                }
                continue; // 不监听本应用、密码框与黑名单应用
            }
            let trimmed = info.text.trim().to_string();
            if enter_only {
                // 回车模式只更新暂存快照，不发出翻译事件；快照属于当前这个输入框进程。
                if trimmed.is_empty() || trimmed.chars().count() > 2000 {
                    clear_staged_uia(&state);
                } else if let Ok(mut latest) = state.latest_uia.write() {
                    *latest = Some(StagedText {
                        event: TextEvent {
                            text: trimmed,
                            source: "uia".to_string(),
                            app: info.process_name.clone(),
                        },
                        pid: info.pid,
                    });
                }
                continue;
            }
            if trimmed.chars().count() < 1 || trimmed.chars().count() > 2000 {
                continue;
            }
            if trimmed == last_text {
                continue; // 文本去重：没变化不重复发出
            }
            last_text = trimmed.clone();
            let _ = app.emit(
                "companion-text",
                TextEvent { text: trimmed, source: "uia".to_string(), app: info.process_name },
            );
        }
    });
}

#[cfg(windows)]
fn clear_staged_uia(state: &MonitorState) {
    if let Ok(mut latest) = state.latest_uia.write() {
        *latest = None;
    }
}

#[derive(Debug, Clone)]
#[cfg(windows)]
struct FocusInfo {
    text: String,
    process_name: String,
    pid: u32,
    is_password: bool,
}

/// 读取当前焦点元素的信息。文本允许为空（回车提交后输入框被清空时仍需知道焦点属于哪个进程）；
/// 密码框只返回 is_password 标记，文本恒为空，绝不读取其内容。
#[cfg(windows)]
fn focused_input_info() -> Option<FocusInfo> {
    use uiautomation::patterns::{UITextPattern, UIValuePattern};
    use uiautomation::UIAutomation;

    let automation = UIAutomation::new().ok()?;
    let element = automation.get_focused_element().ok()?;
    let pid = element.get_process_id().unwrap_or(0);
    let process_name = process_name_of(pid);
    if element.is_password().unwrap_or(false) {
        return Some(FocusInfo { text: String::new(), process_name, pid, is_password: true });
    }

    // 优先 ValuePattern（普通输入框），其次 TextPattern（文档型编辑器）
    let mut text = String::new();
    if let Ok(value_pattern) = element.get_pattern::<UIValuePattern>() {
        if let Ok(value) = value_pattern.get_value() {
            text = value;
        }
    }
    if text.trim().is_empty() {
        if let Ok(text_pattern) = element.get_pattern::<UITextPattern>() {
            if let Ok(range) = text_pattern.get_document_range() {
                if let Ok(document_text) = range.get_text(-1) {
                    text = document_text;
                }
            }
        }
    }
    Some(FocusInfo { text, process_name, pid, is_password: false })
}

// ---------- 回车触发 / Esc 清除（仅观察按键状态，不拦截） ----------

#[cfg(not(windows))]
fn start_key_trigger_monitor(_app: AppHandle, _state: MonitorState) {}

#[cfg(windows)]
fn start_key_trigger_monitor(app: AppHandle, state: MonitorState) {
    std::thread::spawn(move || {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_RETURN};

        let own_pid = std::process::id();
        let mut enter_down = false;
        let mut escape_down = false;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(40));
            let enter_now = unsafe { GetAsyncKeyState(i32::from(VK_RETURN.0)) } < 0;
            let escape_now = unsafe { GetAsyncKeyState(i32::from(VK_ESCAPE.0)) } < 0;

            if escape_now && !escape_down {
                let _ = app.emit("clear-display", ());
            }
            if enter_now && !enter_down {
                emit_staged_uia_on_enter(&app, &state, own_pid);
            }
            enter_down = enter_now;
            escape_down = escape_now;
        }
    });
}

#[cfg(windows)]
fn emit_staged_uia_on_enter(app: &AppHandle, state: &MonitorState, own_pid: u32) {
    if !state.enter_only.load(Ordering::Relaxed)
        || state.paused.load(Ordering::Relaxed)
        || !state.uia_enabled.load(Ordering::Relaxed)
    {
        return;
    }
    // 回车可能让宿主软件立即提交并清空输入框，所以这里只用焦点信息确认：
    // 按回车时焦点仍属于暂存文本的那个进程，且不是密码框/本应用/黑名单。
    let Some(info) = focused_input_info() else {
        return;
    };
    if info.pid == 0 || info.pid == own_pid || info.is_password || state.is_blacklisted(&info.process_name) {
        return;
    }
    let staged = state.latest_uia.read().ok().and_then(|latest| latest.clone());
    if let Some(staged) = staged {
        if staged.pid == info.pid {
            let _ = app.emit("companion-text", staged.event);
        }
    }
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
