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
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
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
    /// 进程名缓存，避免每次 UIA 轮询都 OpenProcess 查询。
    pub process_names: Arc<RwLock<HashMap<u32, String>>>,
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
            process_names: Arc::new(RwLock::new(HashMap::new())),
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
    /// UIA 元素标识（RuntimeId 优先）：快照必须属于同一个输入框，而不只是同一个进程。
    pub element_id: String,
    pub captured_at: Instant,
}

/// 按回车瞬间的焦点快照（平台无关，便于单测回车判定）。
#[derive(Debug, Clone)]
pub struct FocusSnapshot {
    pub pid: u32,
    pub element_id: String,
    pub process_name: String,
    pub is_password: bool,
}

/// 暂存快照的有效期：UIA 持续聚焦时会不断刷新；读取中断超过这个时间就视为过期。
pub const STAGED_TEXT_TTL: Duration = Duration::from_secs(5);

/// 回车触发判定：焦点必须仍是暂存文本所属的同一个输入框，且快照未过期。
pub fn staged_text_for_enter(
    staged: Option<&StagedText>,
    focus: &FocusSnapshot,
    own_pid: u32,
    focus_allowed: bool,
    now: Instant,
) -> Option<TextEvent> {
    if focus.pid == 0 || focus.pid == own_pid || focus.is_password || !focus_allowed {
        return None;
    }
    let staged = staged?;
    if staged.pid != focus.pid
        || staged.element_id.is_empty()
        || staged.element_id != focus.element_id
        || now.duration_since(staged.captured_at) > STAGED_TEXT_TTL
    {
        return None;
    }
    Some(staged.event.clone())
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
            let Some(info) = focused_input_info(&state) else {
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
                        element_id: info.element_id.clone(),
                        captured_at: Instant::now(),
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
    element_id: String,
    is_password: bool,
}

#[cfg(windows)]
fn element_identity(element: &uiautomation::UIElement) -> String {
    if let Ok(runtime_id) = element.get_runtime_id() {
        if !runtime_id.is_empty() {
            return runtime_id.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");
        }
    }
    // 少数控件没有 RuntimeId 时退回 AutomationId + 类名组合（仍比只看进程精确）。
    let automation_id = element.get_automation_id().unwrap_or_default();
    let class_name = element.get_classname().unwrap_or_default();
    format!("{}|{}", automation_id, class_name)
}

#[cfg(windows)]
fn process_name_cached(state: &MonitorState, pid: u32) -> String {
    if pid == 0 {
        return String::new();
    }
    if let Ok(cache) = state.process_names.read() {
        if let Some(name) = cache.get(&pid) {
            return name.clone();
        }
    }
    let name = process_name_of(pid);
    if !name.is_empty() {
        if let Ok(mut cache) = state.process_names.write() {
            cache.insert(pid, name.clone());
        }
    }
    name
}

/// 读取当前焦点元素的信息。文本允许为空（回车提交后输入框被清空时仍需知道焦点属于哪个进程）；
/// 密码框只返回 is_password 标记，文本恒为空，绝不读取其内容。
#[cfg(windows)]
fn focused_input_info(state: &MonitorState) -> Option<FocusInfo> {
    use uiautomation::patterns::{UITextPattern, UIValuePattern};
    use uiautomation::UIAutomation;

    let automation = UIAutomation::new().ok()?;
    let element = automation.get_focused_element().ok()?;
    let pid = element.get_process_id().unwrap_or(0);
    let process_name = process_name_cached(state, pid);
    let element_id = element_identity(&element);
    if element.is_password().unwrap_or(false) {
        return Some(FocusInfo { text: String::new(), process_name, pid, element_id, is_password: true });
    }

    // 优先 ValuePattern（普通输入框）
    let mut text = String::new();
    if let Ok(value_pattern) = element.get_pattern::<UIValuePattern>() {
        if let Ok(value) = value_pattern.get_value() {
            text = value;
        }
    }
    // TextPattern（文档型编辑器）：先取当前选区，其次取可见范围；
    // 不再无条件读取整篇文档，避免长文档被整段暂存/拒绝。
    if text.trim().is_empty() {
        if let Ok(text_pattern) = element.get_pattern::<UITextPattern>() {
            if let Ok(ranges) = text_pattern.get_selection() {
                for range in ranges {
                    if let Ok(selected) = range.get_text(2001) {
                        if !selected.trim().is_empty() {
                            text = selected;
                            break;
                        }
                    }
                }
            }
            if text.trim().is_empty() {
                if let Ok(ranges) = text_pattern.get_visible_ranges() {
                    if let Some(range) = ranges.first() {
                        if let Ok(visible) = range.get_text(2001) {
                            text = visible;
                        }
                    }
                }
            }
        }
    }
    Some(FocusInfo { text, process_name, pid, element_id, is_password: false })
}

// ---------- 回车触发 / Esc 清除（仅观察按键状态，不拦截） ----------

#[cfg(not(windows))]
fn start_key_trigger_monitor(_app: AppHandle, _state: MonitorState) {}

#[cfg(windows)]
fn start_key_trigger_monitor(app: AppHandle, state: MonitorState) {
    std::thread::spawn(move || {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_RETURN};

        let own_pid = std::process::id();
        // 初始状态取按键当前状态：应用启动时正按住回车/Esc 不应误触发一次。
        let mut enter_down = unsafe { GetAsyncKeyState(i32::from(VK_RETURN.0)) } < 0;
        let mut escape_down = unsafe { GetAsyncKeyState(i32::from(VK_ESCAPE.0)) } < 0;
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
    // 焦点必须仍是暂存文本所属的同一个输入框（进程 + 元素标识一致），且快照未过期。
    let Some(info) = focused_input_info(state) else {
        return;
    };
    let focus = FocusSnapshot {
        pid: info.pid,
        element_id: info.element_id.clone(),
        process_name: info.process_name.clone(),
        is_password: info.is_password,
    };
    let focus_allowed = !state.is_blacklisted(&info.process_name);
    let staged = state.latest_uia.read().ok().and_then(|latest| latest.clone());
    if let Some(event) = staged_text_for_enter(staged.as_ref(), &focus, own_pid, focus_allowed, Instant::now()) {
        let _ = app.emit("companion-text", event);
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

    fn staged(at: Instant) -> StagedText {
        StagedText {
            event: TextEvent { text: "hello".to_string(), source: "uia".to_string(), app: "app.exe".to_string() },
            pid: 42,
            element_id: "1,2,3".to_string(),
            captured_at: at,
        }
    }

    fn focus() -> FocusSnapshot {
        FocusSnapshot { pid: 42, element_id: "1,2,3".to_string(), process_name: "app.exe".to_string(), is_password: false }
    }

    #[test]
    fn enter_uses_snapshot_only_for_same_element() {
        let now = Instant::now();
        let s = staged(now);
        assert!(staged_text_for_enter(Some(&s), &focus(), 7, true, now).is_some());
        let mut other = focus();
        other.element_id = "9,9".to_string();
        assert!(staged_text_for_enter(Some(&s), &other, 7, true, now).is_none());
    }

    #[test]
    fn enter_rejects_expired_password_and_own_process() {
        let now = Instant::now();
        let s = staged(now - STAGED_TEXT_TTL - Duration::from_millis(1));
        assert!(staged_text_for_enter(Some(&s), &focus(), 7, true, now).is_none());
        let fresh = staged(now);
        let mut password = focus();
        password.is_password = true;
        assert!(staged_text_for_enter(Some(&fresh), &password, 7, true, now).is_none());
        assert!(staged_text_for_enter(Some(&fresh), &focus(), 42, true, now).is_none());
        assert!(staged_text_for_enter(Some(&fresh), &focus(), 7, false, now).is_none());
    }
}
