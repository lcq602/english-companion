<script setup lang="ts">
// 主窗口：纯显示块。没有输入框——文本来自 UIA 读取当前焦点输入框 / 剪贴板监听
// （Rust monitor 线程发 companion-text 事件），这里只负责防抖翻译并展示译文。
// 全透明小块；控件只在悬停时浮现；位置可在配置窗口里设定，也可直接拖动。
import { computed, onMounted, onUnmounted, ref } from "vue";
import { getCurrentWindow, PhysicalPosition } from "@tauri-apps/api/window";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import WordDetailCard from "./components/WordDetailCard.vue";
import { loadSettings, saveSettings, textColorOf, type AppSettings } from "./lib/settings";
import {
  addFavorite, isTauri, lookupWord, openSettingsWindow, setClickThrough,
  syncMonitorConfig, translateText, type WordDetail,
} from "./lib/api";
import { isMostlyEnglish, splitWords } from "./lib/words";
import "./styles.css";

const POS_KEY = "english-companion.window-pos";

const settings = ref<AppSettings>(loadSettings());
const result = ref("");
const status = ref<"idle" | "loading" | "success" | "error">("idle");
const error = ref("");
const providerName = ref("");
const fromCache = ref(false);
const clickThrough = ref(false);
const paused = ref(settings.value.paused);

const wordDetail = ref<WordDetail | null>(null);
const wordLoading = ref(false);
const wordError = ref("");

let requestId = 0;
let lastTranslatedText = "";
let pendingText = "";
let debounceTimer: number | undefined;
const unlisteners: UnlistenFn[] = [];

const resultTokens = computed(() => (result.value && isMostlyEnglish(result.value) ? splitWords(result.value) : null));
const displayColor = computed(() => textColorOf(settings.value));

async function runTranslate(text: string) {
  const id = ++requestId;
  status.value = "loading";
  error.value = "";
  try {
    const response = await translateText(text, settings.value);
    if (id !== requestId) return;
    result.value = response.translated_text;
    providerName.value = response.provider_name;
    fromCache.value = response.from_cache;
    lastTranslatedText = text;
    status.value = "success";
  } catch (e) {
    if (id !== requestId) return;
    error.value = e instanceof Error ? e.message : String(e);
    status.value = "error";
  }
}

function onIncomingText(text: string) {
  const trimmed = text.trim();
  if (!trimmed) return;
  pendingText = trimmed;
  window.clearTimeout(debounceTimer);
  const delay = Math.min(5000, Math.max(0, settings.value.debounceMs || 0));
  debounceTimer = window.setTimeout(() => {
    if (pendingText && pendingText !== lastTranslatedText) void runTranslate(pendingText);
  }, delay);
}

async function onWordClick(word: string) {
  wordLoading.value = true;
  wordError.value = "";
  wordDetail.value = null;
  try {
    wordDetail.value = await lookupWord(word, settings.value);
  } catch (e) {
    wordError.value = e instanceof Error ? e.message : String(e);
  } finally {
    wordLoading.value = false;
  }
}

async function onFavoriteWord(detail: WordDetail) {
  await addFavorite(detail.word, detail.meaning_zh, detail.phonetic).catch(() => undefined);
}

async function toggleClickThrough() {
  const next = !clickThrough.value;
  clickThrough.value = await setClickThrough(next).catch(() => next);
}

async function togglePause() {
  paused.value = !paused.value;
  settings.value = { ...settings.value, paused: paused.value };
  saveSettings(settings.value);
  await syncMonitorConfig(settings.value).catch(() => undefined);
}

function toggleLock() {
  settings.value = { ...settings.value, lockPosition: !settings.value.lockPosition };
  saveSettings(settings.value);
}

const win = () => (isTauri() ? getCurrentWindow() : null);
function hideWindow() { void win()?.hide().catch(() => undefined); }
function closeWindow() { void win()?.close().catch(() => undefined); }
function startDragging(event: MouseEvent) {
  if (settings.value.lockPosition) return;
  if (event.button !== 0) return;
  if ((event.target as HTMLElement).closest("button")) return;
  void win()?.startDragging().catch(() => undefined);
}

async function applyWindowSettings() {
  if (!isTauri()) return;
  await getCurrentWindow().setAlwaysOnTop(settings.value.alwaysOnTop).catch(() => undefined);
}

onMounted(async () => {
  await syncMonitorConfig(settings.value).catch(() => undefined);
  await applyWindowSettings();
  if (!isTauri()) return;
  const window = getCurrentWindow();

  try {
    const saved = JSON.parse(localStorage.getItem(POS_KEY) ?? "null") as { x: number; y: number } | null;
    if (saved) await window.setPosition(new PhysicalPosition(saved.x, saved.y));
  } catch { /* 忽略位置恢复失败 */ }
  unlisteners.push(await window.onMoved(({ payload }) => {
    localStorage.setItem(POS_KEY, JSON.stringify({ x: payload.x, y: payload.y }));
  }));

  if (settings.value.clickThroughOnStart) {
    clickThrough.value = await setClickThrough(true).catch(() => false);
  }

  unlisteners.push(await listen<{ text: string; source: string; app: string }>("companion-text", (event) => {
    onIncomingText(event.payload.text);
  }));
  unlisteners.push(await listen<boolean>("clickthrough-changed", (event) => {
    clickThrough.value = event.payload;
  }));
  unlisteners.push(await listen<boolean>("monitor-paused-changed", (event) => {
    paused.value = event.payload;
    settings.value = { ...settings.value, paused: event.payload };
    saveSettings(settings.value);
  }));
  unlisteners.push(await listen<AppSettings>("settings-changed", (event) => {
    settings.value = { ...loadSettings(), ...event.payload };
    paused.value = settings.value.paused;
    void applyWindowSettings();
  }));
});

onUnmounted(() => {
  window.clearTimeout(debounceTimer);
  unlisteners.forEach((fn) => fn());
});
</script>

<template>
  <main
    class="desktop-stage display-only"
    :style="{
      '--card-opacity': settings.opacity,
      '--display-color': displayColor.color,
      '--display-stroke': displayColor.stroke,
    }"
  >
    <section
      class="translation-card display-card"
      :class="{ 'is-through': clickThrough, 'has-outline': settings.textOutline }"
      @mousedown="startDragging"
    >
      <div class="hover-controls" @mousedown.stop>
        <span class="hover-buttons">
          <button class="window-button" type="button" @click="togglePause" :title="paused ? '恢复监听' : '暂停监听'">{{ paused ? "▶" : "⏸" }}</button>
          <button class="window-button" type="button" @click="toggleClickThrough" title="点击穿透开关">👆</button>
          <button class="window-button" type="button" @click="toggleLock" :title="settings.lockPosition ? '位置已固定，点击解锁' : '固定窗口位置'">{{ settings.lockPosition ? "🔒" : "📌" }}</button>
          <button class="window-button" type="button" @click="openSettingsWindow" title="打开配置（独立窗口）">⚙</button>
          <button class="window-button" type="button" @click="hideWindow" title="隐藏 (Ctrl+Shift+H)">▽</button>
          <button class="window-button close-button" type="button" @click="closeWindow" title="关闭">×</button>
        </span>
      </div>

      <div class="display-body" aria-live="polite">
        <!-- 默认空白：没有译文时什么都不显示；只有报错才出字，方便定位问题 -->
        <p v-if="error" class="error-message">{{ error }}</p>
        <p v-else-if="resultTokens" class="result-text big">
          <template v-for="(token, i) in resultTokens" :key="i">
            <button v-if="token.isWord" class="word-token" type="button" @click="onWordClick(token.text)" title="点击查释义">{{ token.text }}</button>
            <span v-else>{{ token.text }}</span>
          </template>
        </p>
        <p v-else-if="result" class="result-text big">{{ result }}</p>
      </div>

      <WordDetailCard
        v-if="wordDetail || wordLoading || wordError"
        :detail="wordDetail ?? { word: '', phonetic: '', audio_url: '', meaning_zh: '', definitions: [] }"
        :loading="wordLoading"
        :error="wordError"
        @close="wordDetail = null; wordError = ''"
        @favorite="onFavoriteWord"
      />
    </section>
  </main>
</template>
