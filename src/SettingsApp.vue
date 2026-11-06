<script setup lang="ts">
// 独立配置窗口（index.html#settings）：翻译服务、显示设置、历史与复习、收藏单词。
// 与主显示窗同源共享 localStorage；保存时通过事件通知主窗口即时生效。
import { computed, onMounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { emit } from "@tauri-apps/api/event";
import { loadSettings, saveSettings, PROVIDER_NOTES, type AppSettings, type ProviderId } from "./lib/settings";
import {
  clearHistory, deleteHistory, isTauri, listFavorites, listHistory, removeFavorite, speak,
  type FavoriteWord, type HistoryItem,
} from "./lib/api";
import "./styles.css";

const tab = ref<"service" | "display" | "history" | "words">("service");
const draft = ref<AppSettings>(loadSettings());
const savedHint = ref("");

const providers: { id: ProviderId; label: string }[] = [
  { id: "tencent", label: "腾讯云机器翻译（默认，国内快）" },
  { id: "volcano", label: "火山引擎机器翻译（每月 200 万字符免费，国内快）" },
  { id: "baidu", label: "百度翻译（国内快，需 APP ID）" },
  { id: "google", label: "Google Translate（非官方免费接口）" },
  { id: "microsoft", label: "Microsoft Translator（官方，需 Key）" },
  { id: "mymemory", label: "MyMemory（免费）" },
  { id: "libre", label: "LibreTranslate（可自建）" },
  { id: "appsscript", label: "Google Apps Script 自建翻译（填部署地址）" },
];
const showTencent = computed(() => draft.value.provider === "tencent" || draft.value.fallbackProvider === "tencent");
const showMicrosoft = computed(() => draft.value.provider === "microsoft" || draft.value.fallbackProvider === "microsoft");
const showLibre = computed(() => draft.value.provider === "libre" || draft.value.fallbackProvider === "libre");
const showMyMemory = computed(() => draft.value.provider === "mymemory" || draft.value.fallbackProvider === "mymemory");
const showBaidu = computed(() => draft.value.provider === "baidu" || draft.value.fallbackProvider === "baidu");
const showVolcano = computed(() => draft.value.provider === "volcano" || draft.value.fallbackProvider === "volcano");
const showAppsScript = computed(() => draft.value.provider === "appsscript" || draft.value.fallbackProvider === "appsscript");

async function save() {
  saveSettings(draft.value);
  if (isTauri()) await emit("settings-changed", { ...draft.value }).catch(() => undefined);
  savedHint.value = "已保存，主窗口即时生效";
  window.setTimeout(() => (savedHint.value = ""), 2000);
}

async function closeWindow() {
  if (isTauri()) await getCurrentWindow().hide().catch(() => undefined);
}

// 历史 / 复习
const historyItems = ref<HistoryItem[]>([]);
const reviewing = ref(false);
const reviewIndex = ref(0);
const revealed = ref(false);
const reviewItem = computed(() => historyItems.value[reviewIndex.value] ?? null);
const reviewQuestion = computed(() => {
  const item = reviewItem.value;
  if (!item) return "";
  return item.target_lang === "en" ? item.translated_text : item.source_text;
});
const reviewAnswer = computed(() => {
  const item = reviewItem.value;
  if (!item) return "";
  return item.target_lang === "en" ? item.source_text : item.translated_text;
});
async function refreshHistory() { historyItems.value = await listHistory().catch(() => []); }
async function onRemoveHistory(id: number) { await deleteHistory(id).catch(() => undefined); await refreshHistory(); }
async function onClearHistory() { await clearHistory().catch(() => undefined); historyItems.value = []; }
function formatTime(ts: number) {
  return new Date(ts * 1000).toLocaleString("zh-CN", { hour12: false, month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

// 收藏
const favoriteItems = ref<FavoriteWord[]>([]);
async function refreshFavorites() { favoriteItems.value = await listFavorites().catch(() => []); }
async function onRemoveFavorite(word: string) { await removeFavorite(word).catch(() => undefined); await refreshFavorites(); }

onMounted(async () => {
  document.body.classList.add("settings-body");
  await refreshHistory();
  await refreshFavorites();
  if (isTauri()) {
    // 关闭按钮改为隐藏，避免窗口被销毁后还要重建
    await getCurrentWindow().onCloseRequested(async (event) => {
      event.preventDefault();
      await getCurrentWindow().hide().catch(() => undefined);
    }).catch(() => undefined);
  }
});
</script>

<template>
  <main class="settings-app">
    <header class="settings-header">
      <div>
        <h1>English Companion 设置</h1>
        <p class="settings-sub">配置只保存在本机。翻译走普通机器翻译服务，不使用 AI 大模型。</p>
      </div>
      <button class="secondary-button" type="button" @click="closeWindow">关闭</button>
    </header>

    <nav class="settings-tabs">
      <button type="button" :class="{ active: tab === 'service' }" @click="tab = 'service'">翻译服务</button>
      <button type="button" :class="{ active: tab === 'display' }" @click="tab = 'display'">显示窗口</button>
      <button type="button" :class="{ active: tab === 'history' }" @click="tab = 'history'; refreshHistory()">历史与复习</button>
      <button type="button" :class="{ active: tab === 'words' }" @click="tab = 'words'; refreshFavorites()">收藏单词</button>
    </nav>

    <section v-if="tab === 'service'" class="settings-section">
      <label>
        翻译服务
        <select v-model="draft.provider">
          <option v-for="p in providers" :key="p.id" :value="p.id">{{ p.label }}</option>
        </select>
      </label>
      <p class="note">{{ PROVIDER_NOTES[draft.provider] }}</p>
      <label>
        备用服务（主服务失败时自动切换）
        <select v-model="draft.fallbackProvider">
          <option value="">不启用</option>
          <option v-for="p in providers.filter((p) => p.id !== draft.provider)" :key="p.id" :value="p.id">{{ p.label }}</option>
        </select>
      </label>

      <template v-if="showTencent">
        <h3>腾讯云机器翻译</h3>
        <label>SecretId<input v-model="draft.tencentSecretId" placeholder="AKID..." autocomplete="off" /></label>
        <label>SecretKey<input v-model="draft.tencentSecretKey" type="password" placeholder="腾讯云密钥" autocomplete="off" /></label>
        <label>地域<input v-model="draft.tencentRegion" placeholder="ap-guangzhou" autocomplete="off" /></label>
        <p class="note">密钥在腾讯云控制台「访问管理 → API 密钥管理」创建。每月免费额度用完会报 NoFreeAmount，需在机器翻译控制台开通后付费或购买资源包。</p>
      </template>

      <template v-if="showVolcano">
        <h3>火山引擎机器翻译</h3>
        <label>AccessKey ID<input v-model="draft.volcanoAccessKey" placeholder="AK..." autocomplete="off" /></label>
        <label>Secret AccessKey<input v-model="draft.volcanoSecretKey" type="password" placeholder="火山引擎密钥" autocomplete="off" /></label>
        <label>地域<input v-model="draft.volcanoRegion" placeholder="cn-north-1" autocomplete="off" /></label>
        <p class="note">先在火山引擎控制台开通机器翻译服务，再到「访问控制 → 密钥管理」创建 AccessKey（建议用子用户并授予 TranslateFullAccess 权限）。每月前 200 万字符免费。</p>
      </template>

      <template v-if="showBaidu">
        <h3>百度翻译</h3>
        <label>APP ID<input v-model="draft.baiduAppId" placeholder="百度翻译 APP ID" autocomplete="off" /></label>
        <label>密钥<input v-model="draft.baiduKey" type="password" placeholder="百度翻译密钥" autocomplete="off" /></label>
        <p class="note">到 fanyi-api.baidu.com 注册并开通通用翻译 API，在管理控制台查看 APP ID 和密钥。未认证每月 5 万字符免费；做个人实名认证升高级版后每月 100 万字符免费。</p>
      </template>

      <template v-if="showAppsScript">
        <h3>Google Apps Script 自建翻译</h3>
        <label>部署地址<input v-model="draft.appsScriptUrl" placeholder="https://script.google.com/macros/s/.../exec" autocomplete="off" /></label>
        <label>Token<input v-model="draft.appsScriptToken" type="password" placeholder="脚本里设定的 token" autocomplete="off" /></label>
        <label>本地代理（可选）<input v-model="draft.appsScriptProxy" placeholder="无法直连 Google 时填，如 http://127.0.0.1:4602" autocomplete="off" /></label>
        <p class="note">填部署得到的 /exec 地址，不要填打开后跳转的 googleusercontent 临时链接（那个会过期）。代理只对这个引擎生效。</p>
      </template>

      <template v-if="showMicrosoft">
        <h3>Microsoft Translator</h3>
        <label>API Key<input v-model="draft.microsoftKey" type="password" placeholder="Azure Translator Key" autocomplete="off" /></label>
        <label>区域（可留空）<input v-model="draft.microsoftRegion" placeholder="例如 eastasia" autocomplete="off" /></label>
      </template>

      <template v-if="showLibre">
        <h3>LibreTranslate</h3>
        <label>服务地址<input v-model="draft.libreUrl" placeholder="https://libretranslate.com 或自建地址" autocomplete="off" /></label>
        <label>API Key（自建免 Key 可留空）<input v-model="draft.libreKey" type="password" autocomplete="off" /></label>
      </template>

      <template v-if="showMyMemory">
        <h3>MyMemory</h3>
        <label>邮箱（可选，填了提升免费额度）<input v-model="draft.mymemoryEmail" placeholder="you@example.com" autocomplete="off" /></label>
      </template>

      <label>
        请求超时（秒）：{{ draft.timeoutSecs }}
        <input v-model.number="draft.timeoutSecs" type="range" min="3" max="30" step="1" />
      </label>
    </section>

    <section v-else-if="tab === 'display'" class="settings-section">
      <label>
        透明度：{{ Math.round(draft.opacity * 100) }}%
        <input v-model.number="draft.opacity" type="range" min="0.35" max="1" step="0.01" />
      </label>
      <label class="switch-row"><input v-model="draft.alwaysOnTop" type="checkbox" /><span>始终置顶</span></label>
      <label class="switch-row"><input v-model="draft.lockPosition" type="checkbox" /><span>固定位置（主窗口不可拖动）</span></label>
      <label class="switch-row"><input v-model="draft.clickThroughOnStart" type="checkbox" /><span>启动时开启点击穿透（点击落到后面的窗口；用 Ctrl+Shift+C 或托盘菜单关闭）</span></label>
      <p class="note">快捷键：Ctrl+Shift+T 把剪贴板文本送入翻译 · Ctrl+Shift+H 显示/隐藏 · Ctrl+Shift+C 点击穿透开关。</p>
    </section>

    <section v-else-if="tab === 'history'" class="settings-section">
      <div v-if="reviewing && reviewItem" class="review-body">
        <p class="review-progress">第 {{ reviewIndex + 1 }} / {{ historyItems.length }} 句</p>
        <div class="review-card dark" @click="revealed = !revealed">
          <p class="review-question">{{ reviewQuestion }}</p>
          <p v-if="revealed" class="review-answer">{{ reviewAnswer }}</p>
          <p v-else class="review-hint">点击卡片显示译文</p>
        </div>
        <div class="review-actions">
          <button class="secondary-button" type="button" @click="reviewIndex = (reviewIndex - 1 + historyItems.length) % historyItems.length; revealed = false">上一句</button>
          <button class="secondary-button" type="button" @click="speak(reviewQuestion)">🔊 朗读</button>
          <button class="secondary-button" type="button" @click="reviewIndex = (reviewIndex + 1) % historyItems.length; revealed = false">下一句</button>
        </div>
        <button class="secondary-button wide" type="button" @click="reviewing = false">返回历史列表</button>
      </div>
      <template v-else>
        <div class="settings-row-actions">
          <button class="secondary-button" type="button" :disabled="!historyItems.length" @click="reviewing = true; reviewIndex = 0; revealed = false">开始复习</button>
          <button class="secondary-button danger" type="button" :disabled="!historyItems.length" @click="onClearHistory">清空历史</button>
        </div>
        <p v-if="!historyItems.length" class="empty-note dark">还没有翻译记录。</p>
        <article v-for="item in historyItems" :key="item.id" class="history-item dark">
          <div class="history-main static">
            <span class="history-source">{{ item.source_text }}</span>
            <span class="history-result">{{ item.translated_text }}</span>
            <span class="history-meta">{{ formatTime(item.created_at) }} · {{ item.provider }}</span>
          </div>
          <button class="window-button dark" type="button" @click="onRemoveHistory(item.id)">×</button>
        </article>
      </template>
    </section>

    <section v-else class="settings-section">
      <p v-if="!favoriteItems.length" class="empty-note dark">还没有收藏。在主窗口点击译文中的英文单词，查到释义后点 ★ 收藏。</p>
      <article v-for="item in favoriteItems" :key="item.id" class="word-item dark">
        <div class="word-main">
          <span class="word-text">{{ item.word }}</span>
          <span v-if="item.phonetic" class="word-phonetic">{{ item.phonetic }}</span>
          <span class="word-meaning">{{ item.meaning }}</span>
        </div>
        <button class="window-button dark" type="button" @click="speak(item.word)">🔊</button>
        <button class="window-button dark" type="button" @click="onRemoveFavorite(item.word)">×</button>
      </article>
    </section>

    <footer class="settings-footer">
      <span class="saved-hint">{{ savedHint }}</span>
      <button class="primary-button" type="button" @click="save">保存设置</button>
    </footer>
  </main>
</template>
