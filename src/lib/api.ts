import { invoke } from "@tauri-apps/api/core";
import type { AppSettings } from "./settings";
import { blacklistList, toProviderConfig } from "./settings";

export type TranslateResponse = {
  translated_text: string;
  provider: string;
  provider_name: string;
  source_lang: string;
  target_lang: string;
  from_cache: boolean;
};

export type HistoryItem = {
  id: number;
  source_text: string;
  translated_text: string;
  source_lang: string;
  target_lang: string;
  provider: string;
  created_at: number;
};

export type FavoriteWord = {
  id: number;
  word: string;
  meaning: string;
  phonetic: string;
  created_at: number;
};

export type WordDetail = {
  word: string;
  phonetic: string;
  audio_url: string;
  meaning_zh: string;
  definitions: string[];
};

export const isTauri = () => "__TAURI_INTERNALS__" in window;

export async function translateText(text: string, settings: AppSettings): Promise<TranslateResponse> {
  if (!isTauri()) throw new Error("翻译需要在桌面应用中运行（浏览器预览模式无后端）。");
  return invoke<TranslateResponse>("translate_text", {
    request: { text, source_lang: "auto", target_lang: "auto", config: toProviderConfig(settings) },
  });
}

export async function lookupWord(word: string, settings: AppSettings): Promise<WordDetail> {
  return invoke<WordDetail>("lookup_word", { word, config: toProviderConfig(settings) });
}

export async function listHistory(limit = 200): Promise<HistoryItem[]> {
  if (!isTauri()) return [];
  return invoke<HistoryItem[]>("list_history", { limit });
}

export async function deleteHistory(id: number): Promise<void> {
  if (isTauri()) await invoke("delete_history", { id });
}

export async function clearHistory(): Promise<void> {
  if (isTauri()) await invoke("clear_history");
}

export async function listFavorites(): Promise<FavoriteWord[]> {
  if (!isTauri()) return [];
  return invoke<FavoriteWord[]>("list_favorites");
}

export async function addFavorite(word: string, meaning: string, phonetic: string): Promise<void> {
  if (isTauri()) await invoke("add_favorite", { word, meaning, phonetic });
}

export async function removeFavorite(word: string): Promise<void> {
  if (isTauri()) await invoke("remove_favorite", { word });
}

export async function copyText(text: string): Promise<void> {
  if (isTauri()) {
    await invoke("copy_text", { text });
  } else {
    await navigator.clipboard.writeText(text);
  }
}

export async function syncMonitorConfig(settings: AppSettings): Promise<void> {
  if (!isTauri()) return;
  await invoke("set_monitor_config", {
    config: {
      paused: settings.paused,
      clipboard_enabled: settings.clipboardEnabled,
      uia_enabled: settings.uiaEnabled,
      blacklist: blacklistList(settings),
    },
  });
}

/// 英文发音：优先词典真人音频，其次 WebView 自带语音合成（均为普通系统能力，不涉及大模型）。
export function speak(text: string, audioUrl = ""): void {
  if (audioUrl) {
    const audio = new Audio(audioUrl);
    void audio.play().catch(() => speakWithTts(text));
    return;
  }
  speakWithTts(text);
}

function speakWithTts(text: string): void {
  try {
    const utterance = new SpeechSynthesisUtterance(text);
    utterance.lang = "en-US";
    utterance.rate = 0.92;
    speechSynthesis.cancel();
    speechSynthesis.speak(utterance);
  } catch {
    // 无语音能力时静默失败，不打扰用户
  }
}
