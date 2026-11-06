// English Companion 设置（本地保存，不上传；API Key 只存本机 WebView 本地存储，
// 与原项目保存腾讯云密钥的方式一致，且从不写入日志）。
export type ProviderId = "google" | "microsoft" | "mymemory" | "libre";

export type AppSettings = {
  // 翻译服务
  provider: ProviderId;
  fallbackProvider: ProviderId | "";
  microsoftKey: string;
  microsoftRegion: string;
  libreUrl: string;
  libreKey: string;
  mymemoryEmail: string;
  timeoutSecs: number;
  // 窗口
  alwaysOnTop: boolean;
  opacity: number; // 0.35 - 1，悬浮窗卡片不透明度
  bilingual: boolean; // true=中英对照，false=纯英文
  // 监听
  paused: boolean;
  clipboardEnabled: boolean;
  uiaEnabled: boolean;
  blacklist: string; // 每行一个进程名片段
};

export const DEFAULT_SETTINGS: AppSettings = {
  provider: "google",
  fallbackProvider: "mymemory",
  microsoftKey: "",
  microsoftRegion: "",
  libreUrl: "https://libretranslate.com",
  libreKey: "",
  mymemoryEmail: "",
  timeoutSecs: 10,
  alwaysOnTop: true,
  opacity: 0.92,
  bilingual: true,
  paused: false,
  clipboardEnabled: true,
  uiaEnabled: true,
  blacklist: "1password\nkeepass\nbitwarden\nlastpass",
};

const STORAGE_KEY = "english-companion.settings.v1";

export function loadSettings(): AppSettings {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null") as Partial<AppSettings> | null;
    return { ...DEFAULT_SETTINGS, ...(saved ?? {}) };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

export function saveSettings(settings: AppSettings): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
}

/// 传给 Rust 端的统一服务配置（与 src-tauri/src/translate.rs 的 ProviderConfig 对应）。
export function toProviderConfig(settings: AppSettings) {
  return {
    provider: settings.provider,
    fallback_provider: settings.fallbackProvider,
    microsoft_key: settings.microsoftKey,
    microsoft_region: settings.microsoftRegion,
    libre_url: settings.libreUrl,
    libre_key: settings.libreKey,
    mymemory_email: settings.mymemoryEmail,
    timeout_secs: settings.timeoutSecs,
  };
}

export function blacklistList(settings: AppSettings): string[] {
  return settings.blacklist.split(/\r?\n/).map((s) => s.trim()).filter(Boolean);
}

export const PROVIDER_NOTES: Record<ProviderId, string> = {
  google: "默认。非官方免费网页接口（不是 Google Cloud 官方 API），无需注册、无需 Key，但可能限流或变更，不保证稳定性。",
  microsoft: "官方 API。需自备 Azure Translator Key；免费 F0 额度每月 200 万字符。",
  mymemory: "免费备用。匿名约 1000 词/天；设置里填写邮箱可提升到约 10000 词/天。",
  libre: "开源可自建。填自建服务地址即可免 Key 使用；官方公共站现需付费 Key。",
};
