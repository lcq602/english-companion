<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import SettingsPanel from "./components/SettingsPanel.vue";
import HistoryPanel from "./components/HistoryPanel.vue";
import FavoritesPanel from "./components/FavoritesPanel.vue";
import WordDetailCard from "./components/WordDetailCard.vue";
import { detectDirection } from "./lib/language";
import { loadSettings, saveSettings, type AppSettings } from "./lib/settings";
import {
  addFavorite, clearHistory, copyText, deleteHistory, isTauri, listFavorites, listHistory,
  lookupWord, removeFavorite, speak, syncMonitorConfig, translateText,
  type FavoriteWord, type HistoryItem, type WordDetail,
} from "./lib/api";
import { isMostlyEnglish, splitWords } from "./lib/words";
import "./styles.css";

const DEBOUNCE_MS = 800; // 需求：停止输入 800ms 后触发

const settings = ref<AppSettings>(loadSettings());
const input = ref("");
const result = ref("");
const status = ref<"idle" | "loading" | "success" | "error">("idle");
const error = ref("");
const providerName = ref("");
const fromCache = ref(false);
const inputSource = ref<"" | "uia" | "clipboard" | "manual">("");
const copied = ref(false);

const panel = ref<"none" | "settings" | "history" | "favorites">("none");
const historyItems = ref<HistoryItem[]>([]);
const favoriteItems = ref<FavoriteWord[]>([]);
const stats = ref({ cache_count: 0, history_count: 0 });

const wordDetail = ref<WordDetail | null>(null);
const wordLoading = ref(false);
const wordError = ref("");

let requestId = 0;
let lastRequestedText = "";
let debounceTimer: number | undefined;
const unlisteners: UnlistenFn[] = [];

const detected = computed(() => detectDirection(input.value));
const directionLabel = computed(() => {
  if (detected.value === "to-english") return "中文 → English";
  if (detected.value === "to-chinese") return "English → 中文";
  if (detected.value === "mixed") return "自动识别";
  return "等待输入";
});
const resultTokens = computed(() => (result.value && isMostlyEnglish(result.value) ? splitWords(result.value) : null));
const listenLabel = computed(() => {
  if (settings.value.paused) return "已暂停监听";
  const parts: string[] = [];
  if (settings.value.uiaEnabled) parts.push("输入框");
  if (settings.value.clipboardEnabled) parts.push("剪贴板");
  return parts.length ? `正在监听：${parts.join(" + ")}` : "监听已关闭";
});

async function runTranslate(text: string) {
  const id = ++requestId;
  status.value = "loading";
  error.value = "";
  try {
    const response = await translateText(text, settings.value);
    if (id !== requestId) return; // 防止旧请求覆盖新结果
    result.value = response.translated_text;
    providerName.value = response.provider_name;
    fromCache.value = response.from_cache;
    status.value = "success";
    void refreshStats();
  } catch (e) {
    if (id !== requestId) return;
    error.value = e instanceof Error ? e.message : String(e);
    status.value = "error";
  }
}

// 输入变化：800ms 防抖 + 文本去重（相同文本不重复请求）
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

function applySettings(next: AppSettings) {
  settings.value = next;
  saveSettings(next);
  void syncWindowSettings();
}

async function syncWindowSettings() {
  await syncMonitorConfig(settings.value).catch(() => undefined);
  if (isTauri()) {
    await getCurrentWindow().setAlwaysOnTop(settings.value.alwaysOnTop).catch(() => undefined);
  }
}

watch(settings, () => { saveSettings(settings.value); }, { deep: true });

function togglePaused() {
  settings.value = { ...settings.value, paused: !settings.value.paused };
  saveSettings(settings.value);
  void syncWindowSettings();
}

function clearAll() {
  requestId += 1;
  lastRequestedText = "";
  input.value = "";
  inputSource.value = "";
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
  favoriteItems.value = await listFavorites().catch(() => favoriteItems.value);
}

async function openHistory() {
  historyItems.value = await listHistory().catch(() => []);
  panel.value = "history";
}
async function openFavorites() {
  favoriteItems.value = await listFavorites().catch(() => []);
  panel.value = "favorites";
}
function pickHistory(item: HistoryItem) {
  inputSource.value = "manual";
  input.value = item.source_text;
  panel.value = "none";
}
async function onRemoveHistory(id: number) {
  await deleteHistory(id).catch(() => undefined);
  historyItems.value = await listHistory().catch(() => historyItems.value);
}
async function onClearHistory() {
  await clearHistory().catch(() => undefined);
  historyItems.value = [];
  void refreshStats();
}
async function onRemoveFavorite(word: string) {
  await removeFavorite(word).catch(() => undefined);
  favoriteItems.value = await listFavorites().catch(() => favoriteItems.value);
}

async function refreshStats() {
  if (!isTauri()) return;
  try {
    stats.value = await invoke<{ cache_count: number; history_count: number }>("get_stats");
  } catch { /* 忽略统计失败 */ }
}

const win = () => (isTauri() ? getCurrentWindow() : null);
function minimizeWindow() { void win()?.minimize().catch(() => undefined); }
function hideWindow() { void win()?.hide().catch(() => undefined); }
function closeWindow() { void win()?.close().catch(() => undefined); }
function startDragging(event: MouseEvent) {
  if (event.button !== 0) return;
  if ((event.target as HTMLElement).closest("button")) return;
  void win()?.startDragging().catch(() => undefined);
}

onMounted(async () => {
  await syncWindowSettings();
  await refreshStats();
  if (!isTauri()) return;
  unlisteners.push(await listen<{ text: string; source: string; app: string }>("companion-text", (event) => {
    if (settings.value.paused) return;
    const text = event.payload.text.trim();
    if (!text || text === input.value.trim()) return; // 去重
    inputSource.value = event.payload.source === "uia" ? "uia" : "clipboard";
    input.value = text;
  }));
  unlisteners.push(await listen<boolean>("monitor-paused-changed", (event) => {
    if (settings.value.paused !== event.payload) {
      settings.value = { ...settings.value, paused: event.payload };
      saveSettings(settings.value);
    }
  }));
});

onUnmounted(() => {
  window.clearTimeout(debounceTimer);
  unlisteners.forEach((fn) => fn());
});
</script>

<template>
  <main class="desktop-stage" :style="{ '--card-opacity': settings.opacity }">
    <section class="translation-card">
      <header class="card-header" data-tauri-drag-region @mousedown="startDragging">
        <div class="brand-mark">译</div>
        <div class="brand-copy">
          <h1>English Companion</h1>
          <p class="brand-sub">{{ listenLabel }}</p>
        </div>
        <div class="window-actions" @mousedown.stop>
          <button class="window-button" type="button" @click="togglePaused" :title="settings.paused ? '恢复监听 (Ctrl+Shift+P)' : '暂停监听 (Ctrl+Shift+P)'">{{ settings.paused ? "▶" : "⏸" }}</button>
          <button class="window-button" type="button" @click="settings.bilingual = !settings.bilingual" :title="settings.bilingual ? '当前中英对照，点切换纯英文' : '当前纯英文，点切换中英对照'">{{ settings.bilingual ? "对照" : "纯英" }}</button>
          <button class="window-button" type="button" @click="openHistory" title="历史与复习">🕘</button>
          <button class="window-button" type="button" @click="openFavorites" title="收藏单词">★</button>
          <button class="window-button" type="button" @click="panel = 'settings'" title="设置">⚙</button>
          <button class="window-button" type="button" @click="minimizeWindow" title="最小化">−</button>
          <button class="window-button" type="button" @click="hideWindow" title="隐藏 (Ctrl+Shift+H 显示)">▽</button>
          <button class="window-button close-button" type="button" @click="closeWindow" title="关闭">×</button>
        </div>
      </header>

      <div class="sentence-widget">
        <section v-if="settings.bilingual" class="source-pane">
          <div class="compact-heading">
            <span class="direction-label">{{ directionLabel }}</span>
            <span v-if="inputSource === 'uia'" class="source-tag">来自输入框</span>
            <span v-else-if="inputSource === 'clipboard'" class="source-tag">来自剪贴板</span>
            <span class="character-count">{{ input.length }}</span>
            <button class="clear-button" type="button" :disabled="!input && !result" @click="clearAll" title="清空">×</button>
          </div>
          <textarea
            v-model="input"
            placeholder="在这里输入，或在任何软件里打字 / 复制文本，停顿 0.8 秒自动翻译…"
            rows="3"
            @input="inputSource = 'manual'"
          />
        </section>

        <section class="result-pane">
          <div class="compact-heading">
            <span class="status-indicator" :data-status="status" />
            <span v-if="status === 'loading'" class="loading-mark">翻译中…</span>
            <span v-else-if="providerName" class="provider-label">{{ providerName }}{{ fromCache ? " · 本地缓存" : "" }}</span>
            <span class="result-actions" v-if="result">
              <button class="mini-button" type="button" @click="copyResult">{{ copied ? "已复制" : "复制" }}</button>
              <button class="mini-button" type="button" @click="speak(result)">🔊 朗读</button>
            </span>
          </div>
          <div class="result-panel" :class="status" aria-live="polite">
            <p v-if="error" class="error-message">{{ error }}</p>
            <p v-else-if="resultTokens" class="result-text">
              <template v-for="(token, i) in resultTokens" :key="i">
                <button v-if="token.isWord" class="word-token" type="button" @click="onWordClick(token.text)" title="点击查释义">{{ token.text }}</button>
                <span v-else>{{ token.text }}</span>
              </template>
            </p>
            <p v-else-if="result" class="result-text">{{ result }}</p>
            <p v-else class="empty-note">译文会出现在这里。点击英文单词可以查中文释义和发音。</p>
          </div>
        </section>

        <WordDetailCard
          v-if="wordDetail || wordLoading || wordError"
          :detail="wordDetail ?? { word: '', phonetic: '', audio_url: '', meaning_zh: '', definitions: [] }"
          :loading="wordLoading"
          :error="wordError"
          @close="wordDetail = null; wordError = ''"
          @favorite="onFavoriteWord"
        />
      </div>

      <footer class="card-footer">
        <span>缓存 {{ stats.cache_count }} 条 · 历史 {{ stats.history_count }} 条</span>
        <span class="shortcut-hint">Ctrl+Shift+T 剪贴板翻译 · H 显隐 · P 暂停</span>
      </footer>
    </section>

    <SettingsPanel v-if="panel === 'settings'" :settings="settings" @save="(s) => { applySettings(s); panel = 'none'; }" @close="panel = 'none'" />
    <HistoryPanel v-if="panel === 'history'" :items="historyItems" @close="panel = 'none'" @pick="pickHistory" @remove="onRemoveHistory" @clear="onClearHistory" />
    <FavoritesPanel v-if="panel === 'favorites'" :items="favoriteItems" @close="panel = 'none'" @remove="onRemoveFavorite" />
  </main>
</template>
