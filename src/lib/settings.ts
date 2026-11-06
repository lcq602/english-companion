// English Companion 设置（本地保存，不上传；密钥只存本机 WebView 本地存储，从不写入日志）。
export type ProviderId = "tencent" | "google" | "microsoft" | "mymemory" | "libre" | "baidu" | "volcano" | "appsscript";
export type TriggerMode = "enter" | "auto";

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
  baiduAppId: string;
  baiduKey: string;
  volcanoAccessKey: string;
  volcanoSecretKey: string;
  volcanoRegion: string;
  appsScriptUrl: string;
  appsScriptToken: string;
  appsScriptProxy: string;
  timeoutSecs: number;
  // 显示窗
  alwaysOnTop: boolean;
  opacity: number; // 0.35 - 1，透明显示窗的不透明度
  lockPosition: boolean; // 固定位置（禁止拖动）
  clickThroughOnStart: boolean; // 启动时开启点击穿透
  // 监听（v1.4 恢复：主显示窗没有输入框，文本靠读焦点输入框/剪贴板获得）
  paused: boolean;
  clipboardEnabled: boolean;
  uiaEnabled: boolean;
  triggerMode: TriggerMode; // enter=在输入框按回车才翻译；auto=输入停顿后自动翻译
  blacklist: string; // 每行一个进程名片段
  // 显示文字（v1.5：背景纯透明，文字用醒目高亮色 + 描边保证任何背景下看得清）
  textColorId: string; // TEXT_COLORS 的 id
  textOutline: boolean; // 文字描边/阴影
  debounceMs: number; // 停止输入多久后触发翻译（毫秒）
};

/// 显示块文字颜色预设：都是高醒目色，stroke 是配套描边色（保证在任意背景上都明显）。
export const TEXT_COLORS: { id: string; label: string; color: string; stroke: string }[] = [
  { id: "yellow", label: "荧光黄", color: "#FFE93C", stroke: "rgba(0,0,0,0.72)" },
  { id: "green", label: "荧光绿", color: "#76FF03", stroke: "rgba(0,0,0,0.72)" },
  { id: "orange", label: "亮橙", color: "#FF9100", stroke: "rgba(0,0,0,0.72)" },
  { id: "sky", label: "天蓝", color: "#40C4FF", stroke: "rgba(0,0,0,0.72)" },
  { id: "white", label: "纯白", color: "#FFFFFF", stroke: "rgba(0,0,0,0.78)" },
  { id: "black", label: "纯黑", color: "#141414", stroke: "rgba(255,255,255,0.85)" },
];

export function textColorOf(settings: AppSettings) {
  return TEXT_COLORS.find((c) => c.id === settings.textColorId) ?? TEXT_COLORS[0];
}

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
  baiduAppId: "",
  baiduKey: "",
  volcanoAccessKey: "",
  volcanoSecretKey: "",
  volcanoRegion: "cn-north-1",
  appsScriptUrl: "",
  appsScriptToken: "",
  appsScriptProxy: "",
  timeoutSecs: 10,
  alwaysOnTop: true,
  opacity: 0.92,
  lockPosition: false,
  clickThroughOnStart: false,
  paused: false,
  clipboardEnabled: true,
  uiaEnabled: true,
  triggerMode: "enter",
  blacklist: "1password\nkeepass\nbitwarden\nlastpass",
  textColorId: "yellow",
  textOutline: true,
  debounceMs: 800,
};

// ---- 存储（v1.6 起）：普通设置与密钥分开存 ----
// 普通设置存 settings.v2（已剔除密钥字段）；密钥打包后经 Rust 端 Windows DPAPI
// 加密，只保存密文（secrets.v1）。旧版 settings.v1（明文）在首次启动时自动迁移。
const STORAGE_KEY = "english-companion.settings.v2";
const LEGACY_STORAGE_KEY = "english-companion.settings.v1";
const SECRETS_KEY = "english-companion.secrets.v1";

export const SECRET_FIELDS = [
  "tencentSecretId", "tencentSecretKey", "microsoftKey", "libreKey",
  "baiduAppId", "baiduKey", "volcanoAccessKey", "volcanoSecretKey", "appsScriptToken",
] as const;
type SecretField = (typeof SECRET_FIELDS)[number];

let secretCache: Partial<Record<SecretField, string>> | null = null;
let hydrated = false;

function extractSecrets(settings: AppSettings): Partial<Record<SecretField, string>> {
  const out: Partial<Record<SecretField, string>> = {};
  for (const field of SECRET_FIELDS) {
    const value = settings[field];
    if (value) out[field] = value;
  }
  return out;
}

function sanitize(settings: AppSettings): AppSettings {
  const copy = { ...settings };
  for (const field of SECRET_FIELDS) copy[field] = "";
  return copy;
}

export function loadSettings(): AppSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY) ?? localStorage.getItem(LEGACY_STORAGE_KEY);
    const saved = JSON.parse(raw ?? "null") as Partial<AppSettings> | null;
    const merged = { ...DEFAULT_SETTINGS, ...(saved ?? {}) };
    if (secretCache) {
      for (const field of SECRET_FIELDS) merged[field] = secretCache[field] ?? "";
    }
    return merged;
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

/// 同步保存：只写普通设置；密钥字段只做"非空合并"进内存缓存并异步加密落盘，
/// 因此主窗口的暂停/穿透等小开关保存不会误删已存密钥。
export function saveSettings(settings: AppSettings): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(sanitize(settings)));
  const incoming = extractSecrets(settings);
  if (Object.keys(incoming).length) {
    secretCache = { ...(secretCache ?? {}), ...incoming };
    void persistSecrets(secretCache);
  }
}

async function persistSecrets(secrets: Partial<Record<SecretField, string>>): Promise<void> {
  if (!("__TAURI_INTERNALS__" in window)) return;
  if (!Object.keys(secrets).length) {
    localStorage.removeItem(SECRETS_KEY);
    return;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  const ciphertext = await invoke<string>("protect_secrets", { plaintext: JSON.stringify(secrets) });
  localStorage.setItem(SECRETS_KEY, ciphertext);
}

/// 配置窗口保存专用：密钥以草稿为准全量替换（清空输入框即删除对应密钥），
/// 等待 DPAPI 加密真正完成才返回，失败会抛错给界面提示。
export async function saveSettingsSecure(settings: AppSettings): Promise<void> {
  secretCache = extractSecrets(settings);
  localStorage.setItem(STORAGE_KEY, JSON.stringify(sanitize(settings)));
  await persistSecrets(secretCache);
}

/// 启动时调用一次：解密已存密钥到内存，并把旧版明文设置迁移到新存储。
export async function hydrateSettings(): Promise<AppSettings> {
  if (hydrated) return loadSettings();
  const base = loadSettings();
  const legacySecrets = extractSecrets(base);
  let storedSecrets: Partial<Record<SecretField, string>> = {};
  const blob = localStorage.getItem(SECRETS_KEY);
  if (blob && "__TAURI_INTERNALS__" in window) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const plaintext = await invoke<string>("unprotect_secrets", { ciphertext: blob });
      storedSecrets = JSON.parse(plaintext) as Partial<Record<SecretField, string>>;
    } catch {
      storedSecrets = {}; // 密文不可解（如换了电脑/用户）：不覆盖原密文，密钥按未配置处理
    }
  }
  secretCache = { ...storedSecrets, ...legacySecrets };
  if (Object.keys(legacySecrets).length) {
    // 旧版明文迁移：写入加密存储 + 普通设置改存脱敏版 + 删除旧明文
    try {
      await persistSecrets(secretCache);
      localStorage.setItem(STORAGE_KEY, JSON.stringify(sanitize(base)));
      localStorage.removeItem(LEGACY_STORAGE_KEY);
    } catch {
      // 加密不可用时保留旧数据，不丢用户密钥
    }
  }
  hydrated = true;
  return loadSettings();
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
    baidu_app_id: settings.baiduAppId,
    baidu_key: settings.baiduKey,
    volcano_access_key: settings.volcanoAccessKey,
    volcano_secret_key: settings.volcanoSecretKey,
    volcano_region: settings.volcanoRegion,
    apps_script_url: settings.appsScriptUrl,
    apps_script_token: settings.appsScriptToken,
    apps_script_proxy: settings.appsScriptProxy,
  };
}

export function blacklistList(settings: AppSettings): string[] {
  return settings.blacklist
    .split(/\r?\n/)
    .map((line) => line.trim().toLowerCase())
    .filter(Boolean);
}

export const PROVIDER_NOTES: Record<ProviderId, string> = {
  tencent: "腾讯云机器翻译 TMT（默认）：国内直连、速度快。每月有免费额度，额度用完需在腾讯云控制台开通后付费或购买资源包。需要 SecretId / SecretKey。",
  google: "非官方免费网页接口（不是 Google Cloud 官方 API），无需注册、无需 Key，但国内网络可能很慢或连不上，且可能限流。",
  microsoft: "官方 API。需自备 Azure Translator Key；免费 F0 额度每月 200 万字符。",
  mymemory: "免费服务。匿名约 1000 词/天；设置里填写邮箱可提升到约 10000 词/天。",
  libre: "开源可自建。填自建服务地址即可免 Key 使用；官方公共站现需付费 Key。",
  baidu: "百度翻译开放平台：国内直连、速度快。需在 fanyi-api.baidu.com 注册拿 APP ID + 密钥；未认证每月 5 万字符免费，个人实名认证升高级版后每月 100 万字符免费，超出 49 元/百万字符。",
  volcano: "火山引擎机器翻译（字节跳动）：国内直连、速度快。需在火山引擎控制台开通机器翻译并创建 AccessKey；每月前 200 万字符免费，超出约 49 元/百万字符，免费额度是目前最慷慨的。",
  appsscript: "你自己部署的 Google Apps Script 翻译服务：填部署地址（以 /exec 结尾）和脚本里设定的 Token。走 Google 服务器，本地无法直连时可在下方填本地代理地址（如 http://127.0.0.1:4602）。注意要填 /exec 部署地址，不要填打开后跳转的 googleusercontent 临时链接。",
};
