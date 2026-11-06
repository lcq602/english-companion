// English Companion 设置（本地保存，不上传；密钥只存本机 WebView 本地存储，从不写入日志）。
export type ProviderId = "tencent" | "google" | "microsoft" | "mymemory" | "libre";

export type AppSettings = {
  // 翻译服务
  provider: ProviderId;
  fallbackProvider: ProviderId | "";
  tencentSecretId: string;
  tencentSecretKey: string;
  tencentRegion: string;
  microsoftKey: string;
  microsoftRegion: string;
  libreUrl: string;
  libreKey: string;
  mymemoryEmail: string;
  timeoutSecs: number;
  // 显示窗
  alwaysOnTop: boolean;
  opacity: number; // 0.35 - 1，透明显示窗的不透明度
  lockPosition: boolean; // 固定位置（禁止拖动）
  clickThroughOnStart: boolean; // 启动时开启点击穿透
};

export const DEFAULT_SETTINGS: AppSettings = {
  provider: "tencent",
  fallbackProvider: "google",
  tencentSecretId: "",
  tencentSecretKey: "",
  tencentRegion: "ap-guangzhou",
  microsoftKey: "",
  microsoftRegion: "",
  libreUrl: "https://libretranslate.com",
  libreKey: "",
  mymemoryEmail: "",
  timeoutSecs: 10,
  alwaysOnTop: true,
  opacity: 0.92,
  lockPosition: false,
  clickThroughOnStart: false,
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
    tencent_secret_id: settings.tencentSecretId,
    tencent_secret_key: settings.tencentSecretKey,
    tencent_region: settings.tencentRegion,
  };
}

export const PROVIDER_NOTES: Record<ProviderId, string> = {
  tencent: "腾讯云机器翻译 TMT（默认）：国内直连、速度快。每月有免费额度，额度用完需在腾讯云控制台开通后付费或购买资源包。需要 SecretId / SecretKey。",
  google: "非官方免费网页接口（不是 Google Cloud 官方 API），无需注册、无需 Key，但国内网络可能很慢或连不上，且可能限流。",
  microsoft: "官方 API。需自备 Azure Translator Key；免费 F0 额度每月 200 万字符。",
  mymemory: "免费服务。匿名约 1000 词/天；设置里填写邮箱可提升到约 10000 词/天。",
  libre: "开源可自建。填自建服务地址即可免 Key 使用；官方公共站现需付费 Key。",
};
