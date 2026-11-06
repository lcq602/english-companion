# English Companion 英语伴侣（免费翻译版）

Windows 桌面实时英语翻译伴侣：在任何软件里打字（或复制文本），停顿 0.8 秒后，屏幕右侧的透明悬浮窗自动给出英文译文，并可点词查释义、发音、收藏与复习。

> **本项目不使用 AI 大模型**：没有 OpenAI / DeepSeek / Claude 等任何大模型接口，只使用普通机器翻译服务。原项目前端由 React 迁移为 Vue 3，保留了原有悬浮窗设计与语言检测逻辑。

## 技术栈

- Tauri 2 + Rust（翻译引擎、SQLite、Windows UI Automation、剪贴板监听、全局快捷键、系统托盘）
- Vue 3 + TypeScript + Vite
- SQLite（rusqlite）：翻译缓存 / 历史 / 单词收藏，存于应用数据目录 `english-companion.db`

## 翻译引擎（统一 TranslationProvider 接口）

- **默认：腾讯云机器翻译 TMT**——国内直连、延迟低。在独立配置窗口填写自己的 SecretId / SecretKey（保存在本机，不上传、不进日志）。每月有免费额度，额度用完会返回 NoFreeAmount，需在腾讯云控制台开通后付费或购买资源包；可在配置里开备用服务自动接管。
- **火山引擎机器翻译（v1.2 新增）**——国内直连、速度快，免费额度最慷慨：每月前 200 万字符免费，超出约 49 元/百万字符。需在火山引擎控制台开通机器翻译并创建 AccessKey。
- **百度翻译（v1.2 新增）**——国内直连、速度快。需在 fanyi-api.baidu.com 注册拿 APP ID + 密钥；未认证每月 5 万字符免费，个人实名认证升高级版后每月 100 万字符免费，超出 49 元/百万字符。
- Google Translate：非官方免费网页接口（**不是** Google Cloud 官方 Translation API，也不保证稳定/可用），无需 Key；**国内网络可能很慢或连不上**，因为它走境外服务器直连（网页版走代理所以感觉不到）。请求超时可调、失败自动重试 1 次，并可在主/备之间自动切换。
- Microsoft Translator：官方 API，需用户自己的 Azure Key（F0 免费层每月 200 万字符）。
- MyMemory：免费备用。匿名约 1000 词/天；填邮箱可提升到约 10000 词/天。
- LibreTranslate：开源可自建，配置自建地址即可免 Key；公共站需付费 Key。
- 中译英 / 英译中自动识别；技术名词（Spring Boot、Vue、Rust 等）用占位保护不被误译；同一文本先查 SQLite 缓存，不重复请求。

## 功能（v1.1 极简显示窗）

- **主窗口就是一块透明显示屏**：输入或粘贴文字，停顿 0.8 秒自动出译文；没有自动监听输入框/剪贴板的功能了（v1.0 的 UIA 与剪贴板轮询已按反馈移除）。
- **点击穿透**：点 👆 按钮或 Ctrl+Shift+C 开启，点击会落到窗口后面的应用；再用快捷键或托盘菜单关闭。可在配置里设为启动时开启。
- **位置固定**：点 📌 锁定位置（不可拖动）；窗口位置会自动记住，下次启动回到原地。
- **配置独立成窗**：主窗口点 ⚙ 或托盘「打开配置」，在单独窗口里改翻译服务、密钥、透明度、置顶，顺带管理历史/复习/收藏单词。
- 点译文里的英文单词查中文释义、音标、发音（免费词典接口 + 机器翻译补中文义）；历史与收藏仍保存在本机 SQLite。
- 快捷键：Ctrl+Shift+T 把剪贴板文本送入翻译（手动触发）· Ctrl+Shift+H 显示/隐藏 · Ctrl+Shift+C 点击穿透开关。托盘图标单击显隐，右键菜单含打开配置/穿透开关/退出。

## 安全与隐私

- 不监听键盘、不读取其他软件输入框、不自动读剪贴板；只翻译你输入/粘贴或用快捷键主动送入的文本。
- 密钥只保存在本机应用本地存储，不上传、不写入日志、不进仓库。

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
- CI：推送到 GitHub 后 Actions（windows-2022）自动测试并打包，便携版与安装版发布到 Release `v1.2.0`。

需要 Node.js 20+、Rust stable、Visual C++ Build Tools、WebView2 Runtime（安装包会引导安装）。

## 开发阶段完成情况

1. ✅ 桌面悬浮窗（透明/无边框/置顶/右侧/可拖动/透明度/双模式/显隐）
2. ✅ 剪贴板快捷键翻译（`Ctrl+Shift+T` + 剪贴板监听），验证翻译引擎
3. ✅ Google + Microsoft + MyMemory + LibreTranslate 统一引擎（重试/超时/故障转移）；v1.2 新增百度翻译与火山引擎机器翻译（国内直连、免费额度更大）
4. ✅ Windows UI Automation 自动获取输入框内容（密码框跳过、黑名单、暂停）
5. ✅ SQLite 缓存、历史、单词收藏、句子复习、点词释义与发音
6. ✅ GitHub Actions 在 Windows 上构建与兼容性验证（Win10 22H2 目标），NSIS 安装包 + 便携 exe
