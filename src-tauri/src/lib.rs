//! English Companion 主入口（v1.4）：主窗口是纯显示块（无输入框，全透明，只显示译文），
//! 文本来源是 UIA 读取当前焦点输入框（密码框跳过、黑名单、可暂停）与剪贴板监听；
//! 配置（服务/监听/显示位置）在独立窗口。翻译走普通机器翻译服务，不用 AI 大模型。

mod db;
mod lang;
mod monitor;
mod secrets;
mod terms;
mod translate;

use db::{Db, FavoriteWord, HistoryItem};
use monitor::MonitorState;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
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
    /// 限制并发翻译请求：最多同时 2 个在途请求。
    semaphore: Arc<Semaphore>,
    /// 主窗口是否处于点击穿透状态（点击落到窗口后面的应用）。
    click_through: Arc<AtomicBool>,
}

/// 缓存作用域：把翻译配置（引擎/备用引擎/区域/服务地址/账号标识与密钥的哈希）
/// 一并纳入缓存 key。切换引擎或账号后不会再命中旧引擎的缓存；作用域本身是
/// SHA-256 哈希，不保存任何明文配置。
fn cache_scope(config: &ProviderConfig) -> String {
    let mut normalized = config.clone();
    normalized.timeout_secs = 0; // 超时不影响译文，不纳入作用域
    let serialized = serde_json::to_string(&normalized).unwrap_or_default();
    db::cache_key(&serialized, "config", "v1")
}

fn scoped_cache_key(text: &str, source: &str, target: &str, config: &ProviderConfig) -> String {
    db::cache_key(&format!("{}|{}", cache_scope(config), text), source, target)
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

    // 1) 先查本地 SQLite 缓存（作用域含当前引擎配置，切换引擎不会串缓存）
    let key = scoped_cache_key(&text, &source, &target, &request.config);
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
    let client = translate::http_client(8)?;

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

    // 中文释义走机器翻译（优先缓存，作用域同翻译缓存）
    let key = scoped_cache_key(&word, "en", "zh", &config);
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

#[tauri::command]
fn clear_cache(state: tauri::State<'_, AppState>) {
    state.db.cache_clear();
}

/// 用 Windows DPAPI 加密一包密钥 JSON（前端只保存返回的密文）。
#[tauri::command]
fn protect_secrets(plaintext: String) -> Result<String, String> {
    secrets::protect(&plaintext)
}

/// 解密 protect_secrets 保存的密文。
#[tauri::command]
fn unprotect_secrets(ciphertext: String) -> Result<String, String> {
    secrets::unprotect(&ciphertext)
}

// ---------- 监听配置 / 窗口 / 剪贴板命令 ----------

#[derive(Debug, Clone, Deserialize)]
pub struct MonitorConfig {
    pub paused: bool,
    pub clipboard_enabled: bool,
    pub uia_enabled: bool,
    pub enter_only: bool,
    pub blacklist: Vec<String>,
}

#[tauri::command]
fn set_monitor_config(state: tauri::State<'_, AppState>, config: MonitorConfig) {
    state.monitor.paused.store(config.paused, Ordering::Relaxed);
    state.monitor.clipboard_enabled.store(config.clipboard_enabled, Ordering::Relaxed);
    state.monitor.uia_enabled.store(config.uia_enabled, Ordering::Relaxed);
    state.monitor.enter_only.store(config.enter_only, Ordering::Relaxed);
    if let Ok(mut list) = state.monitor.blacklist.write() {
        *list = config.blacklist;
    }
}

/// 把主显示窗移动到指定物理坐标（配置窗口调用）。
#[tauri::command]
fn move_main_window(app: tauri::AppHandle, x: i32, y: i32) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
        let _ = window.show();
    }
}

/// 把主显示窗移动到屏幕某个方位：top-right | top-left | bottom-right | bottom-left | center。
#[tauri::command]
fn move_main_window_preset(app: tauri::AppHandle, preset: String) {
    if let Some(window) = app.get_webview_window("main") {
        if let (Ok(Some(monitor)), Ok(size)) = (window.current_monitor(), window.outer_size()) {
            let screen = monitor.size();
            let margin = 24i32;
            let (w, h) = (size.width as i32, size.height as i32);
            let (sw, sh) = (screen.width as i32, screen.height as i32);
            let (x, y) = match preset.as_str() {
                "top-left" => (margin, margin),
                "bottom-left" => (margin, (sh - h - margin).max(margin)),
                "bottom-right" => ((sw - w - margin).max(margin), (sh - h - margin).max(margin)),
                "center" => ((sw - w) / 2, (sh - h) / 2),
                _ => ((sw - w - margin).max(margin), margin), // top-right 默认
            };
            let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
            let _ = window.show();
        }
    }
}

/// 按内容高度调整主显示窗高度（长译文自动长高，短内容自动收回），
/// 并保证窗口不超出当前屏幕范围。宽度与用户摆放的位置保持不变。
#[tauri::command]
fn resize_main_window_height(app: tauri::AppHandle, height: u32) {
    if let Some(window) = app.get_webview_window("main") {
        let Ok(size) = window.outer_size() else { return };
        let Ok(position) = window.outer_position() else { return };
        let screen_height = window
            .current_monitor()
            .ok()
            .flatten()
            .map(|m| m.size().height)
            .unwrap_or(1080);
        let max_height = screen_height.saturating_sub(32).max(160);
        let target = height.clamp(120, max_height);
        if size.height == target {
            return;
        }
        let _ = window.set_size(tauri::PhysicalSize::new(size.width, target));
        // 变高后若底部超出屏幕，把窗口上移收回屏幕内
        if position.y > 0 && (position.y as u32).saturating_add(target) > screen_height {
            let y = (screen_height - target) as i32 - 16;
            let _ = window.set_position(tauri::PhysicalPosition::new(position.x, y.max(0)));
        }
    }
}

/// 显示独立配置窗口：已有则显示聚焦，被关闭销毁后按相同参数重建。
fn show_settings(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let _ = tauri::WebviewWindowBuilder::new(
        app,
        "settings",
        tauri::WebviewUrl::App("index.html#settings".into()),
    )
    .title("English Companion 设置")
    .inner_size(560.0, 700.0)
    .center()
    .build();
}

#[tauri::command]
fn open_settings(app: tauri::AppHandle) {
    show_settings(&app);
}

/// 切换主窗口点击穿透；返回切换后的状态。穿透时点击落到后面的应用，
/// 只能用快捷键 Ctrl+Shift+C 或托盘菜单关掉。
#[tauri::command]
fn set_click_through(app: tauri::AppHandle, state: tauri::State<'_, AppState>, enabled: bool) -> bool {
    state.click_through.store(enabled, Ordering::Relaxed);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_ignore_cursor_events(enabled);
    }
    let _ = app.emit("clickthrough-changed", enabled);
    enabled
}

#[tauri::command]
fn copy_text(state: tauri::State<'_, AppState>, text: String) -> Result<(), String> {
    if let Ok(mut until) = state.monitor.suppress_clipboard_until.lock() {
        *until = std::time::Instant::now() + std::time::Duration::from_secs(2);
    }
    let mut clipboard = arboard::Clipboard::new().map_err(|_| "无法访问剪贴板".to_string())?;
    clipboard.set_text(text).map_err(|_| "复制失败".to_string())
}

/// 快捷键触发：读剪贴板并把文本发给主显示窗走翻译。
/// 监听已暂停时不发出（与托盘/主窗口的暂停语义保持一致）。
fn emit_clipboard_text(app: &tauri::AppHandle, state: &MonitorState) {
    if state.paused.load(Ordering::Relaxed) {
        return;
    }
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
}

fn toggle_pause(app: &tauri::AppHandle, state: &MonitorState) {
    let next = !state.paused.load(Ordering::Relaxed);
    state.paused.store(next, Ordering::Relaxed);
    let _ = app.emit("monitor-paused-changed", next);
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

fn apply_click_through(app: &tauri::AppHandle, state: &AppState, enabled: bool) {
    state.click_through.store(enabled, Ordering::Relaxed);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_ignore_cursor_events(enabled);
    }
    let _ = app.emit("clickthrough-changed", enabled);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let data_dir = app.path().app_data_dir().expect("app data dir");
            std::fs::create_dir_all(&data_dir).ok();
            let db = Arc::new(Db::open(&data_dir.join("english-companion.db"))?);
            let monitor_state = MonitorState::default();
            app.manage(AppState {
                db,
                monitor: monitor_state.clone(),
                semaphore: Arc::new(Semaphore::new(2)),
                click_through: Arc::new(AtomicBool::new(false)),
            });

            // 主窗口定位到屏幕右侧（默认悬浮位置；前端会按保存的位置恢复）
            if let Some(window) = app.get_webview_window("main") {
                if let (Ok(Some(monitor)), Ok(size)) = (window.current_monitor(), window.outer_size()) {
                    let screen = monitor.size();
                    let x = screen.width.saturating_sub(size.width + 24) as i32;
                    let _ = window.set_position(tauri::PhysicalPosition::new(x, 72));
                }
            }

            // 系统托盘：显示/隐藏、暂停监听、配置、点击穿透、退出
            let show_item = MenuItem::with_id(app, "show", "显示 / 隐藏", true, None::<&str>)?;
            let pause_item = MenuItem::with_id(app, "pause", "暂停 / 恢复监听", true, None::<&str>)?;
            let settings_item = MenuItem::with_id(app, "settings", "打开配置", true, None::<&str>)?;
            let through_item = MenuItem::with_id(app, "through", "点击穿透 开/关", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &pause_item, &settings_item, &through_item, &quit_item])?;
            let monitor_for_tray = monitor_state.clone();
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("English Companion 英语伴侣")
                .menu(&menu)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => toggle_window(app),
                    "pause" => toggle_pause(app, &monitor_for_tray),
                    "settings" => show_settings(app),
                    "through" => {
                        if let Some(state) = app.try_state::<AppState>() {
                            let next = !state.click_through.load(Ordering::Relaxed);
                            apply_click_through(app, &state, next);
                        }
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

            // 全局快捷键：T 剪贴板翻译 / H 显示隐藏 / C 点击穿透 / P 暂停监听
            let shortcut_t = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyT);
            let shortcut_h = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyH);
            let shortcut_c = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyC);
            let shortcut_p = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyP);
            let monitor_for_keys = monitor_state.clone();
            app.global_shortcut().on_shortcuts(
                [shortcut_t, shortcut_h, shortcut_c, shortcut_p],
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
                    } else if shortcut == &shortcut_c {
                        if let Some(state) = app.try_state::<AppState>() {
                            let next = !state.click_through.load(Ordering::Relaxed);
                            apply_click_through(app, &state, next);
                        }
                    } else if shortcut == &shortcut_p {
                        toggle_pause(app, &monitor_for_keys);
                    }
                },
            )?;

            // 启动监听线程：UIA 读焦点输入框 + 剪贴板变化（密码框/黑名单/暂停在 monitor 内处理）
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
            clear_cache,
            protect_secrets,
            unprotect_secrets,
            copy_text,
            open_settings,
            set_click_through,
            set_monitor_config,
            move_main_window,
            move_main_window_preset,
            resize_main_window_height
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
