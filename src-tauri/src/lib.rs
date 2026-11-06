//! English Companion 主入口：注册命令、托盘、全局快捷键、监听线程与主窗口（右侧悬浮）。
//! 翻译只走普通机器翻译服务（见 translate 模块），不使用任何 AI 大模型接口。
//! 原腾讯云 TMT 实现已按新需求移除（需要付费密钥、且不在本次指定的服务清单内），
//! 语言检测逻辑保留并移植到了 lang 模块（Rust + TS 双份，保持一致）。

mod db;
mod lang;
mod monitor;
mod terms;
mod translate;

use db::{Db, FavoriteWord, HistoryItem};
use monitor::MonitorState;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tokio::sync::Semaphore;
use translate::ProviderConfig;

pub struct AppState {
    db: Arc<Db>,
    monitor: MonitorState,
    /// 限制并发翻译请求（性能要求）：最多同时 2 个在途请求。
    semaphore: Arc<Semaphore>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TranslateRequest {
    pub text: String,
    pub source_lang: String, // auto | zh | en
    pub target_lang: String, // auto | zh | en
    pub config: ProviderConfig,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranslateResponse {
    pub translated_text: String,
    pub provider: String,
    pub provider_name: String,
    pub source_lang: String,
    pub target_lang: String,
    pub from_cache: bool,
}

#[tauri::command]
async fn translate_text(
    state: tauri::State<'_, AppState>,
    request: TranslateRequest,
) -> Result<TranslateResponse, String> {
    let text = request.text.trim().to_string();
    if text.is_empty() {
        return Err("翻译内容不能为空".to_string());
    }
    if text.chars().count() > 5000 {
        return Err("单次翻译最长 5000 字符，请分段后重试".to_string());
    }
    let source = lang::resolve_source(&text, &request.source_lang);
    let target = if request.target_lang == "zh" || request.target_lang == "en" {
        request.target_lang.clone()
    } else if source == "zh" {
        "en".to_string()
    } else {
        "zh".to_string()
    };
    if source == target {
        return Ok(TranslateResponse {
            translated_text: text.clone(),
            provider: "none".to_string(),
            provider_name: "同语言无需翻译".to_string(),
            source_lang: source,
            target_lang: target,
            from_cache: false,
        });
    }

    // 1) 先查本地 SQLite 缓存（去重：相同文本不重复请求网络）
    let key = db::cache_key(&text, &source, &target);
    if let Some((cached, provider)) = state.db.cache_get(&key) {
        state.db.history_add(&text, &cached, &source, &target, &provider);
        return Ok(TranslateResponse {
            translated_text: cached,
            provider_name: translate::provider_display_name(&provider).to_string(),
            provider,
            source_lang: source,
            target_lang: target,
            from_cache: true,
        });
    }

    // 2) 并发限制后走网络翻译（主服务失败自动切备用）
    let _permit = state
        .semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| "翻译队列已关闭".to_string())?;
    let (translated, provider_used) =
        translate::translate_full(&text, &source, &target, &request.config).await?;

    // 3) 写缓存 + 历史
    state.db.cache_put(&key, &text, &source, &target, &provider_used, &translated);
    state.db.history_add(&text, &translated, &source, &target, &provider_used);

    Ok(TranslateResponse {
        translated_text: translated,
        provider_name: translate::provider_display_name(&provider_used).to_string(),
        provider: provider_used,
        source_lang: source,
        target_lang: target,
        from_cache: false,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct WordDetail {
    pub word: String,
    pub phonetic: String,
    pub audio_url: String,
    pub meaning_zh: String,
    pub definitions: Vec<String>,
}

/// 点击英文单词查释义：Free Dictionary（免费、无需 Key）取音标/英文释义，
/// 再用当前翻译配置把单词译成中文。不使用大模型。
#[tauri::command]
async fn lookup_word(
    state: tauri::State<'_, AppState>,
    word: String,
    config: ProviderConfig,
) -> Result<WordDetail, String> {
    let word = word.trim().to_lowercase();
    if word.is_empty() || !word.chars().all(|c| c.is_ascii_alphabetic() || c == '-' || c == '\'') {
        return Err("请选择一个英文单词".to_string());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|_| "无法创建网络客户端".to_string())?;

    let mut phonetic = String::new();
    let mut audio_url = String::new();
    let mut definitions = Vec::new();
    if let Ok(resp) = client
        .get(format!("https://api.dictionaryapi.dev/api/v2/entries/en/{}", word))
        .send()
        .await
    {
        if let Ok(body) = resp.json::<serde_json::Value>().await {
            if let Some(entry) = body.get(0) {
                phonetic = entry
                    .get("phonetic")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if let Some(phonetics) = entry.get("phonetics").and_then(serde_json::Value::as_array) {
                    for p in phonetics {
                        if phonetic.is_empty() {
                            if let Some(t) = p.get("text").and_then(serde_json::Value::as_str) {
                                phonetic = t.to_string();
                            }
                        }
                        if audio_url.is_empty() {
                            if let Some(a) = p.get("audio").and_then(serde_json::Value::as_str) {
                                if !a.is_empty() {
                                    audio_url = a.to_string();
                                }
                            }
                        }
                    }
                }
                if let Some(meanings) = entry.get("meanings").and_then(serde_json::Value::as_array) {
                    for meaning in meanings {
                        if let Some(defs) = meaning.get("definitions").and_then(serde_json::Value::as_array) {
                            for d in defs.iter().take(2) {
                                if let Some(def) = d.get("definition").and_then(serde_json::Value::as_str) {
                                    definitions.push(def.to_string());
                                }
                            }
                        }
                        if definitions.len() >= 3 {
                            break;
                        }
                    }
                }
            }
        }
    }

    // 中文释义走机器翻译（优先缓存）
    let key = db::cache_key(&word, "en", "zh");
    let meaning_zh = if let Some((cached, _)) = state.db.cache_get(&key) {
        cached
    } else {
        match translate::translate_full(&word, "en", "zh", &config).await {
            Ok((translated, provider_used)) => {
                state.db.cache_put(&key, &word, "en", "zh", &provider_used, &translated);
                translated
            }
            Err(_) => String::new(),
        }
    };

    if phonetic.is_empty() && definitions.is_empty() && meaning_zh.is_empty() {
        return Err("没有查到这个单词，请检查网络连接".to_string());
    }
    Ok(WordDetail { word, phonetic, audio_url, meaning_zh, definitions })
}

// ---------- 历史 / 收藏命令 ----------

#[tauri::command]
fn list_history(state: tauri::State<'_, AppState>, limit: i64) -> Vec<HistoryItem> {
    state.db.history_list(limit.clamp(1, 500))
}

#[tauri::command]
fn delete_history(state: tauri::State<'_, AppState>, id: i64) {
    state.db.history_delete(id);
}

#[tauri::command]
fn clear_history(state: tauri::State<'_, AppState>) {
    state.db.history_clear();
}

#[tauri::command]
fn add_favorite(state: tauri::State<'_, AppState>, word: String, meaning: String, phonetic: String) {
    state.db.favorite_add(&word, &meaning, &phonetic);
}

#[tauri::command]
fn remove_favorite(state: tauri::State<'_, AppState>, word: String) {
    state.db.favorite_remove(&word);
}

#[tauri::command]
fn list_favorites(state: tauri::State<'_, AppState>) -> Vec<FavoriteWord> {
    state.db.favorite_list()
}

#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub cache_count: i64,
    pub history_count: i64,
}

#[tauri::command]
fn get_stats(state: tauri::State<'_, AppState>) -> Stats {
    let (cache_count, history_count) = state.db.cache_stats();
    Stats { cache_count, history_count }
}

// ---------- 监听配置 / 剪贴板命令 ----------

#[derive(Debug, Clone, Deserialize)]
pub struct MonitorConfig {
    pub paused: bool,
    pub clipboard_enabled: bool,
    pub uia_enabled: bool,
    pub blacklist: Vec<String>,
}

#[tauri::command]
fn set_monitor_config(state: tauri::State<'_, AppState>, config: MonitorConfig) {
    state.monitor.paused.store(config.paused, Ordering::Relaxed);
    state.monitor.clipboard_enabled.store(config.clipboard_enabled, Ordering::Relaxed);
    state.monitor.uia_enabled.store(config.uia_enabled, Ordering::Relaxed);
    if let Ok(mut list) = state.monitor.blacklist.write() {
        *list = config.blacklist;
    }
}

#[tauri::command]
fn copy_text(state: tauri::State<'_, AppState>, text: String) -> Result<(), String> {
    // 标记抑制窗口，避免自己复制译文又被剪贴板监听当成新输入
    if let Ok(mut until) = state.monitor.suppress_clipboard_until.lock() {
        *until = std::time::Instant::now() + std::time::Duration::from_secs(2);
    }
    let mut clipboard = arboard::Clipboard::new().map_err(|_| "无法访问剪贴板".to_string())?;
    clipboard.set_text(text).map_err(|_| "复制失败".to_string())
}

/// 主动读取剪贴板并翻译（快捷键 Ctrl+Shift+T 触发时 Rust 直接发事件，前端走同一流程）。
fn emit_clipboard_text(app: &tauri::AppHandle, state: &MonitorState) {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        if let Ok(text) = cb.get_text() {
            let trimmed = text.trim().to_string();
            if trimmed.chars().count() >= 2 {
                let _ = app.emit(
                    "companion-text",
                    monitor::TextEvent { text: trimmed, source: "clipboard".to_string(), app: String::new() },
                );
            }
        }
    }
    let _ = state; // 保持签名简洁，暂停判断在前端与监听线程已处理
}

fn toggle_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            // 数据库：应用数据目录
            let data_dir = app.path().app_data_dir().expect("app data dir");
            std::fs::create_dir_all(&data_dir).ok();
            let db = Arc::new(Db::open(&data_dir.join("english-companion.db"))?);
            let monitor_state = MonitorState::default();
            app.manage(AppState {
                db,
                monitor: monitor_state.clone(),
                semaphore: Arc::new(Semaphore::new(2)),
            });

            // 主窗口定位到屏幕右侧（默认悬浮位置）
            if let Some(window) = app.get_webview_window("main") {
                if let (Ok(Some(monitor)), Ok(size)) = (window.current_monitor(), window.outer_size()) {
                    let screen = monitor.size();
                    let x = screen.width.saturating_sub(size.width + 24) as i32;
                    let _ = window.set_position(tauri::PhysicalPosition::new(x, 72));
                }
            }

            // 系统托盘：显示/隐藏、暂停/恢复、退出
            let show_item = MenuItem::with_id(app, "show", "显示 / 隐藏", true, None::<&str>)?;
            let pause_item = MenuItem::with_id(app, "pause", "暂停 / 恢复监听", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &pause_item, &quit_item])?;
            let monitor_for_tray = monitor_state.clone();
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("English Companion 英语伴侣")
                .menu(&menu)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => toggle_window(app),
                    "pause" => {
                        let paused = monitor_for_tray.paused.load(Ordering::Relaxed);
                        monitor_for_tray.paused.store(!paused, Ordering::Relaxed);
                        let _ = app.emit("monitor-paused-changed", !paused);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        toggle_window(tray.app_handle());
                    }
                })
                .build(app)?;

            // 全局快捷键：Ctrl+Shift+T 剪贴板翻译 / Ctrl+Shift+H 显示隐藏 / Ctrl+Shift+P 暂停恢复
            let shortcut_t = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyT);
            let shortcut_h = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyH);
            let shortcut_p = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyP);
            let monitor_for_keys = monitor_state.clone();
            app.global_shortcut().on_shortcuts(
                [shortcut_t, shortcut_h, shortcut_p],
                move |app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    if shortcut == &shortcut_t {
                        emit_clipboard_text(app, &monitor_for_keys);
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                        }
                    } else if shortcut == &shortcut_h {
                        toggle_window(app);
                    } else if shortcut == &shortcut_p {
                        let paused = monitor_for_keys.paused.load(Ordering::Relaxed);
                        monitor_for_keys.paused.store(!paused, Ordering::Relaxed);
                        let _ = app.emit("monitor-paused-changed", !paused);
                    }
                },
            )?;

            // 启动剪贴板 + UIA 监听线程
            monitor::start_monitors(app.handle().clone(), monitor_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            translate_text,
            lookup_word,
            list_history,
            delete_history,
            clear_history,
            add_favorite,
            remove_favorite,
            list_favorites,
            get_stats,
            set_monitor_config,
            copy_text
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
