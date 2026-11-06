# 透明液态玻璃 UI 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 375x260 翻译小组件从深色磨砂视觉调整为高透明、带缓慢流动高光和轻微交互反馈的液态玻璃视觉。

**Architecture:** 保持 `TranslationCard` 和 `ConfigMenu` 的现有 Props、事件处理、窗口 API 和腾讯云调用不变。通过 `src/styles.css` 的半透明渐层、伪元素、背景模糊和 CSS 动画完成材质变化；修改记录单独追加本次迭代。

**Tech Stack:** React、TypeScript、Tauri 2、CSS backdrop-filter、CSS animations。

## Global Constraints

- 不能改动公共 API。
- 性能下降不能超过 5%。
- 保持 375x260 小组件尺寸、拖动、关闭、缩小、固定、配置和自动翻译行为。
- 界面文字保持中文，代码文件使用 UTF-8 且不产生乱码。
- 每次改动追加到 `doc/修改记录.md`。

---

### Task 1: 重塑玻璃材质和背景光影

**Files:**
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: 现有 `.desktop-stage`、`.translation-card`、`.translation-card::before` 和 `.translation-card::after` 类名。
- Produces: 透明卡片、缓慢表面高光、内部焦散和低对比度边缘反射。

- [ ] 将舞台背景改为透明叠加的动态光影，降低深蓝色实底的存在感。
- [ ] 将玻璃卡片背景改为低不透明度渐层，保留 `backdrop-filter`，增加非线性高光和焦散伪元素。
- [ ] 增加 `@keyframes liquid-glance` 和 `@keyframes caustic-drift`，动画周期保持较长，避免持续高频重绘。

### Task 2: 调整悬浮元素和输入区域

**Files:**
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: `TranslationCard.tsx` 的现有 `.brand-mark`、`.window-button`、`.result-panel`、`textarea`、`.copy-button` 和 `.clear-button` 类名。
- Produces: 蚀刻/悬浮风格图标、轻量分割线、圆润玻璃输入凹槽和 hover/focus ripple 反馈。

- [ ] 将品牌图标改为低饱和金属玻璃块，标题改为清晰的浅色蚀刻文字。
- [ ] 将窗口控制和复制/清空按钮改为细线低亮度按钮，保留可点击区域和现有 aria 标签。
- [ ] 降低结果区分割线对比度，调整输入框的透明度、圆角、内高光和 focus ripple。
- [ ] 在 `prefers-reduced-motion: reduce` 下关闭新增动画。

### Task 3: 记录并验证

**Files:**
- Modify: `doc/修改记录.md`

**Interfaces:**
- Consumes: 完成后的 CSS 视觉改动。
- Produces: 第 15 次修改记录和可构建的前端资源。

- [ ] 追加日期、修改内容和迭代次数。
- [ ] 运行 `npm.cmd run build`。
- [ ] 运行 `cargo +stable check --manifest-path src-tauri\\Cargo.toml`，使用 `E:\\Rust` 环境变量。
