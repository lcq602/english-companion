# English Companion 英语伴侣（免费翻译版）

Windows 桌面实时英语翻译伴侣：在任何软件里打字（或复制文本），停顿 0.8 秒后，屏幕右侧的透明悬浮窗自动给出英文译文，并可点词查释义、发音、收藏与复习。

> **本项目不使用 AI 大模型**：没有 OpenAI / DeepSeek / Claude 等任何大模型接口，只使用普通机器翻译服务。原项目的腾讯云 TMT 已移除（不在指定清单内且需付费密钥）；原有悬浮窗设计、语言检测逻辑得到保留，前端按需求由 React 迁移为 Vue 3。

## 技术栈

- Tauri 2 + Rust（翻译引擎、SQLite、Windows UI Automation、剪贴板监听、全局快捷键、系统托盘）
- Vue 3 + TypeScript + Vite
- SQLite（rusqlite）：翻译缓存 / 历史 / 单词收藏，存于应用数据目录 `english-companion.db`

## 翻译引擎（统一 TranslationProvider 接口）

| 服务 | 说明 | 限制 |
| --- | --- | --- |
| Google Translate（默认） | **非官方免费网页接口**（`client=gtx`），不是 Google Cloud 官方稳定 API | 无需注册/Key；可能限流（429）或变更，不保证稳定性 |
| Microsoft Translator | 官方 API，用户自备 Azure Key | 免费 F0 额度每月 200 万字符 |
| MyMemory（默认备用） | 免费接口 | 匿名约 1000 词/天；设置填邮箱约 10000 词/天 |
| LibreTranslate | 开源，可自建 | 填自建地址免 Key；官方公共站现需付费 Key |

主服务失败（网络异常 / 限流 / 5xx）自动重试一次，再切备用服务，并给出清晰的中文错误提示。所有服务使用统一请求/响应结构，请求带超时（默认 10 秒，可调）。

## 功能

- **悬浮窗**：透明无边框、始终置顶、默认屏幕右侧、可拖动、透明度可调、中英对照 / 纯英文切换、托盘与快捷键显隐。
- **实时翻译**：Windows UI Automation 监听当前焦点输入框 + 剪贴板监听；800ms 防抖、文本去重、SQLite 缓存、并发上限 2、旧请求不覆盖新结果。
- **双向翻译**：自动识别中/英，中译英、英译中；Java、Spring Boot、Vue、Python 等技术名词自动保护不被译坏；长句自动分块。
- **学习**：点英文单词查中文释义/音标/英文释义、发音（词典音频 + 系统语音）、收藏单词、历史记录、句子复习模式（先看英文回忆中文）。
- **快捷键**：`Ctrl+Shift+T` 剪贴板翻译、`Ctrl+Shift+H` 显示/隐藏、`Ctrl+Shift+P` 暂停/恢复监听。

## 安全与隐私

- **密码框绝不捕获**（UIA `IsPassword` 直接跳过，连内容都不读）。
- **应用黑名单**：默认含 1Password / KeePass / Bitwarden / LastPass，可在设置中增改。
- **一键暂停**：界面按钮、托盘菜单、`Ctrl+Shift+P` 均可。
- 默认不上传、不保存无关文本；**不记录原始键盘输入日志**，本地只存实际翻译过的缓存与历史。

## 开发与测试

```cmd
npm install
npm run tauri dev     :: 开发运行
npm test              :: 前端单测（语言检测 / 分词）
cargo test --manifest-path src-tauri/Cargo.toml   :: Rust 单测（解析/缓存/历史/名词保护/UIA 黑名单）
npm run build         :: 前端类型检查 + 构建
```

## 打包（Windows 10 22H2 / Windows 11）

- 本地：双击 `build.bat`，产物为便携版 `english-companion.exe` 与 NSIS 安装包。
- CI：推送到 GitHub 后 Actions（windows-2022）自动测试并打包，便携版与安装版发布到 Release `v1.0.0`。

需要 Node.js 20+、Rust stable、Visual C++ Build Tools、WebView2 Runtime（安装包会引导安装）。

## 开发阶段完成情况

1. ✅ 桌面悬浮窗（透明/无边框/置顶/右侧/可拖动/透明度/双模式/显隐）
2. ✅ 剪贴板快捷键翻译（`Ctrl+Shift+T` + 剪贴板监听），验证翻译引擎
3. ✅ Google + Microsoft + MyMemory + LibreTranslate 统一引擎（重试/超时/故障转移）
4. ✅ Windows UI Automation 自动获取输入框内容（密码框跳过、黑名单、暂停）
5. ✅ SQLite 缓存、历史、单词收藏、句子复习、点词释义与发音
6. ✅ GitHub Actions 在 Windows 上构建与兼容性验证（Win10 22H2 目标），NSIS 安装包 + 便携 exe
