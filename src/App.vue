<script setup lang="ts">
// 主窗口：极简透明「显示屏」。只有手动文字翻译（输入/粘贴 -> 停顿 0.8 秒出译文），
// 没有自动监听。配置在独立窗口（open_settings）。支持点击穿透与位置固定。
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { getCurrentWindow, PhysicalPosition } from "@tauri-apps/api/window";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import WordDetailCard from "./components/WordDetailCard.vue";
import { loadSettings, saveSettings, type AppSettings } from "./lib/settings";
import {
  addFavorite, copyText, isTauri, lookupWord, openSettingsWindow, setClickThrough,
  speak, translateText, type WordDetail,
} from "./lib/api";
import { isMostlyEnglish, splitWords } from "./lib/words";
import "./styles.css";

const DEBOUNCE_MS = 800;
const POS_KEY = "english-companion.window-pos";

const settings = ref<AppSettings>(loadSettings());
const input = ref("");
const result = ref("");
const status = ref<"idle" | "loading" | "success" | "error">("idle");
const error = ref("");
const providerName = ref("");
const fromCache = ref(false);
const copied = ref(false);
const clickThrough = ref(false);

const wordDetail = ref<WordDetail | null>(null);
const wordLoading = ref(false);
const wordError = ref("");

let requestId = 0;
let lastRequestedText = "";
let debounceTimer: number | undefined;
const unlisteners: UnlistenFn[] = [];

const resultTokens = computed(() => (result.value && isMostlyEnglish(result.value) ? splitWords(result.value) : null));

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
    status.value = "success";
  } catch (e) {
    if (id !== requestId) return;
    error.value = e instanceof Error ? e.message : String(e);
    status.value = "error";
  }
}

watch(input, (value) => {
  const text = value.trim();
  window.clearTimeout(debounceTimer);
  if (!text) {
    requestId += 1;
    lastRequestedText = "";
    result.value = "";
    error.value = "";
    status.value = "idle";
    return;
  }
  if (text === lastRequestedText && status.value === "success") return;
  debounceTimer = window.setTimeout(() => {
    if (text === lastRequestedText && status.value === "success") return;
    lastRequestedText = text;
    void runTranslate(text);
  }, DEBOUNCE_MS);
});

function clearAll() {
  requestId += 1;
  lastRequestedText = "";
  input.value = "";
  wordDetail.value = null;
}

async function copyResult() {
  if (!result.value) return;
  await copyText(result.value).catch(() => undefined);
  copied.value = true;
  window.setTimeout(() => (copied.value = false), 1400);
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

function toggleLock() {
  settings.value = { ...settings.value, lockPosition: !settings.value.lockPosition };
  saveSettings(settings.value);
}

const win = () => (isTauri() ? getCurrentWindow() : null);
function hideWindow() { void win()?.hide().catch(() => undefined); }
function closeWindow() { void win()?.close().catch(() => undefined); }
function startDragging(event: MouseEvent) {
  if (settings.value.lockPosition) return; // 位置已固定
  if (event.button !== 0) return;
  if ((event.target as HTMLElement).closest("button")) return;
  void win()?.startDragging().catch(() => undefined);
}

async function applyWindowSettings() {
  if (!isTauri()) return;
  await getCurrentWindow().setAlwaysOnTop(settings.value.alwaysOnTop).catch(() => undefined);
}

onMounted(async () => {
  await applyWindowSettings();
  if (!isTauri()) return;
  const window = getCurrentWindow();

  // 恢复上次位置（默认由 Rust 定位到屏幕右侧）
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

  unlisteners.push(await listen<{ text: string; source: string }>("companion-text", (event) => {
    const text = event.payload.text.trim();
    if (text && text !== input.value.trim()) input.value = text;
  }));
  unlisteners.push(await listen<boolean>("clickthrough-changed", (event) => {
    clickThrough.value = event.payload;
  }));
  unlisteners.push(await listen<AppSettings>("settings-changed", (event) => {
    settings.value = { ...loadSettings(), ...event.payload };
    void applyWindowSettings();
  }));
});

onUnmounted(() => {
  window.clearTimeout(debounceTimer);
  unlisteners.forEach((fn) => fn());
});
</script>

<template>
  <main class="desktop-stage" :style="{ '--card-opacity': settings.opacity }">
    <section class="translation-card" :class="{ 'is-through': clickThrough }">
      <header class="card-header" data-tauri-drag-region @mousedown="startDragging">
        <div class="brand-mark">译</div>
        <span v-if="clickThrough" class="through-badge">点击穿透中 · Ctrl+Shift+C 关闭</span>
        <div class="window-actions" @mousedown.stop>
          <button class="window-button" type="button" @click="toggleClickThrough" :title="clickThrough ? '关闭点击穿透' : '开启点击穿透（点击落到后面的窗口）'">👆</button>
          <button class="window-button" type="button" @click="toggleLock" :title="settings.lockPosition ? '位置已固定，点击解锁' : '固定窗口位置'">{{ settings.lockPosition ? "🔒" : "📌" }}</button>
          <button class="window-button" type="button" @click="openSettingsWindow" title="打开配置（独立窗口）">⚙</button>
          <button class="window-button" type="button" @click="hideWindow" title="隐藏 (Ctrl+Shift+H)">▽</button>
          <button class="window-button close-button" type="button" @click="closeWindow" title="关闭">×</button>
        </div>
      </header>

      <textarea
        v-model="input"
        class="display-input"
        placeholder="输入或粘贴文字，停顿 0.8 秒自动翻译…"
        rows="2"
      />

      <div class="display-result" aria-live="polite">
        <div class="compact-heading">
          <span class="status-indicator" :data-status="status" />
          <span v-if="status === 'loading'" class="loading-mark">翻译中…</span>
          <span v-else-if="providerName" class="provider-label">{{ providerName }}{{ fromCache ? " · 缓存" : "" }}</span>
          <span class="result-actions" v-if="result">
            <button class="mini-button" type="button" @click="copyResult">{{ copied ? "已复制" : "复制" }}</button>
            <button class="mini-button" type="button" @click="speak(result)">🔊</button>
            <button class="mini-button" type="button" @click="clearAll">清空</button>
          </span>
        </div>
        <p v-if="error" class="error-message">{{ error }}</p>
        <p v-else-if="resultTokens" class="result-text">
          <template v-for="(token, i) in resultTokens" :key="i">
            <button v-if="token.isWord" class="word-token" type="button" @click="onWordClick(token.text)" title="点击查释义">{{ token.text }}</button>
            <span v-else>{{ token.text }}</span>
          </template>
        </p>
        <p v-else-if="result" class="result-text">{{ result }}</p>
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
