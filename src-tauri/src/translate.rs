//! 统一翻译引擎：TranslationProvider 接口 + 4 个普通机器翻译服务实现。
//! 不使用任何 AI 大模型接口（无 OpenAI / DeepSeek / Claude 等）。
//!
//! - google    : Google Translate 非官方免费网页接口（client=gtx）。注意：这不是 Google Cloud
//!               官方稳定 API，可能限流或变更，仅作为免费默认引擎，界面与文档均有明确说明。
//! - microsoft : Microsoft Translator 官方 API，需要用户自备 API Key（免费额度 F0 每月 200 万字符）。
//! - mymemory  : MyMemory 免费接口，匿名约 1000 词/天，填邮箱可提升到约 10000 词/天。
//! - libre     : LibreTranslate，用户自建服务地址（默认官方演示站，新版需 Key），开源可自托管。
//! - baidu     : 百度翻译开放平台，国内直连。需 APP ID + 密钥；未认证每月 5 万字符免费，
//!               个人实名认证升高级版后每月 100 万字符免费，超出 49 元/百万字符。
//! - volcano   : 火山引擎机器翻译（字节跳动），国内直连。需 AccessKey + SecretKey；
//!               每月前 200 万字符免费，超出约 49 元/百万字符。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::terms;

/// 按超时秒数复用 reqwest 客户端（连接池），避免每次翻译都新建客户端。
pub fn http_client(timeout_secs: u64) -> Result<reqwest::Client, String> {
    static CLIENTS: OnceLock<Mutex<HashMap<u64, reqwest::Client>>> = OnceLock::new();
    let clients = CLIENTS.get_or_init(|| Mutex::new(HashMap::new()));
    let timeout_secs = timeout_secs.clamp(3, 30);
    if let Ok(guard) = clients.lock() {
        if let Some(client) = guard.get(&timeout_secs) {
            return Ok(client.clone());
        }
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|_| "无法创建网络客户端".to_string())?;
    if let Ok(mut guard) = clients.lock() {
        guard.insert(timeout_secs, client.clone());
    }
    Ok(client)
}

/// 错误重试分类：集中维护，避免各处靠零散字符串判断。
/// 配置缺失/格式错误/额度耗尽重试也没用，直接失败或走备用引擎；
/// 超时/连接失败/限流/5xx 才值得重试一次。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryClass {
    Retryable,
    Fatal,
}

pub fn retry_class(message: &str) -> RetryClass {
    const FATAL_MARKERS: &[&str] = &[
        "尚未配置", "格式不对", "额度", "用完", "NoFreeAmount", "未知的翻译服务", "无法创建",
    ];
    if FATAL_MARKERS.iter().any(|m| message.contains(m)) {
        return RetryClass::Fatal;
    }
    const RETRYABLE_MARKERS: &[&str] = &["超时", "无法连接", "429", "HTTP 5", "请求失败"];
    if RETRYABLE_MARKERS.iter().any(|m| message.contains(m)) {
        return RetryClass::Retryable;
    }
    RetryClass::Fatal
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider: String,          // tencent | google | microsoft | mymemory | libre | baidu | volcano
    pub fallback_provider: String, // 同上，空字符串表示不启用备用
    pub microsoft_key: String,
    pub microsoft_region: String,
    pub libre_url: String,
    pub libre_key: String,
    pub mymemory_email: String,
    pub timeout_secs: u64,
    // 腾讯云 TMT（v1.1 起为默认引擎，用户在独立配置窗口填写，仅存本机）
    #[serde(default)]
    pub tencent_secret_id: String,
    #[serde(default)]
    pub tencent_secret_key: String,
    #[serde(default = "default_tencent_region")]
    pub tencent_region: String,
    // 百度翻译开放平台（v1.2 新增）：APP ID + 密钥
    #[serde(default)]
    pub baidu_app_id: String,
    #[serde(default)]
    pub baidu_key: String,
    // 火山引擎机器翻译（v1.2 新增）：AccessKey + SecretKey
    #[serde(default)]
    pub volcano_access_key: String,
    #[serde(default)]
    pub volcano_secret_key: String,
    #[serde(default = "default_volcano_region")]
    pub volcano_region: String,
    // Google Apps Script 自建翻译（v1.3 新增）：用户自己的 Apps Script 部署地址（/exec）
    #[serde(default)]
    pub apps_script_url: String,
    #[serde(default)]
    pub apps_script_token: String,
    /// 可选本地代理（如 http://127.0.0.1:4602），仅该引擎使用；本地无法直连 Google 时填。
    #[serde(default)]
    pub apps_script_proxy: String,
}

fn default_volcano_region() -> String {
    "cn-north-1".to_string()
}

fn default_tencent_region() -> String {
    "ap-guangzhou".to_string()
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            provider: "tencent".to_string(),
            fallback_provider: "google".to_string(),
            microsoft_key: String::new(),
            microsoft_region: String::new(),
            libre_url: "https://libretranslate.com".to_string(),
            libre_key: String::new(),
            mymemory_email: String::new(),
            timeout_secs: 10,
            tencent_secret_id: String::new(),
            tencent_secret_key: String::new(),
            tencent_region: default_tencent_region(),
            baidu_app_id: String::new(),
            baidu_key: String::new(),
            volcano_access_key: String::new(),
            volcano_secret_key: String::new(),
            volcano_region: default_volcano_region(),
            apps_script_url: String::new(),
            apps_script_token: String::new(),
            apps_script_proxy: String::new(),
        }
    }
}

pub fn provider_display_name(id: &str) -> &'static str {
    match id {
        "tencent" => "腾讯云机器翻译",
        "google" => "Google Translate（非官方免费接口）",
        "microsoft" => "Microsoft Translator",
        "mymemory" => "MyMemory（免费备用）",
        "libre" => "LibreTranslate",
        "baidu" => "百度翻译",
        "volcano" => "火山引擎机器翻译",
        "appsscript" => "Google Apps Script 自建翻译",
        _ => "未知服务",
    }
}

fn lang_for(provider: &str, code: &str) -> String {
    // 内部统一使用 zh / en；各服务对中文代码的要求不同
    match (provider, code) {
        ("google", "zh") => "zh-CN".to_string(),
        ("mymemory", "zh") => "zh-CN".to_string(),
        _ => code.to_string(),
    }
}

/// 单个服务的翻译实现（统一输入输出）。
#[async_trait::async_trait]
pub trait TranslationProvider: Send + Sync {
    fn id(&self) -> &'static str;
    async fn translate(
        &self,
        client: &reqwest::Client,
        text: &str,
        source: &str,
        target: &str,
        config: &ProviderConfig,
    ) -> Result<String, String>;
}

pub struct GoogleProvider;
pub struct MicrosoftProvider;
pub struct MyMemoryProvider;
pub struct LibreProvider;
pub struct TencentProvider;
pub struct BaiduProvider;
pub struct VolcanoProvider;
pub struct AppsScriptProvider;

pub fn make_provider(id: &str) -> Option<Box<dyn TranslationProvider>> {
    match id {
        "tencent" => Some(Box::new(TencentProvider)),
        "google" => Some(Box::new(GoogleProvider)),
        "microsoft" => Some(Box::new(MicrosoftProvider)),
        "mymemory" => Some(Box::new(MyMemoryProvider)),
        "libre" => Some(Box::new(LibreProvider)),
        "baidu" => Some(Box::new(BaiduProvider)),
        "volcano" => Some(Box::new(VolcanoProvider)),
        "appsscript" => Some(Box::new(AppsScriptProvider)),
        _ => None,
    }
}

// ---------- 响应解析（纯函数，可单测） ----------

pub fn parse_google(body: &Value) -> Result<String, String> {
    let segments = body
        .get(0)
        .and_then(Value::as_array)
        .ok_or_else(|| "Google 接口返回格式异常，请稍后重试".to_string())?;
    let mut out = String::new();
    for seg in segments {
        if let Some(s) = seg.get(0).and_then(Value::as_str) {
            out.push_str(s);
        }
    }
    if out.trim().is_empty() {
        return Err("Google 接口没有返回译文".to_string());
    }
    Ok(out)
}

pub fn parse_mymemory(body: &Value) -> Result<String, String> {
    let status = body
        .get("responseStatus")
        .and_then(|v| v.as_str().map(|s| s.to_string()).or_else(|| v.as_i64().map(|n| n.to_string())))
        .unwrap_or_default();
    let quota = body
        .get("responseDetails")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_uppercase();
    if status == "429" || quota.contains("QUOTA") || quota.contains("LIMIT") {
        return Err("MyMemory 免费额度已用完（匿名约 1000 词/天），可在设置里填写邮箱提升额度，或切换其他服务".to_string());
    }
    let text = body
        .get("responseData")
        .and_then(|d| d.get("translatedText"))
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "MyMemory 没有返回译文".to_string())?;
    if text.to_uppercase().contains("QUERY LENGTH LIMIT") {
        return Err("MyMemory 单次文本过长，请缩短后重试".to_string());
    }
    Ok(text.to_string())
}

pub fn parse_libre(body: &Value) -> Result<String, String> {
    if let Some(err) = body.get("error").and_then(Value::as_str) {
        return Err(format!("LibreTranslate 返回错误：{}", err));
    }
    body.get("translatedText")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| "LibreTranslate 没有返回译文".to_string())
}

pub fn parse_tencent(body: &Value) -> Result<String, String> {
    let response = body
        .get("Response")
        .ok_or_else(|| "腾讯云返回格式异常".to_string())?;
    if let Some(error) = response.get("Error") {
        let code = error.get("Code").and_then(Value::as_str).unwrap_or("Unknown");
        let message = error.get("Message").and_then(Value::as_str).unwrap_or("未知错误");
        let hint = match code {
            "FailedOperation.NoFreeAmount" => {
                "：本月免费额度已用完，请到腾讯云机器翻译控制台开通后付费或购买资源包，也可以先切换到备用服务"
            }
            "AuthFailure.SignatureFailure" | "AuthFailure.SecretIdNotFound" => {
                "：请到配置窗口检查 SecretId / SecretKey 是否填写正确"
            }
            "FailedOperation.ServiceIsolate" => "：账号欠费或服务已停用，请到腾讯云控制台检查",
            "RequestLimitExceeded" => "：请求太频繁，请稍后再试",
            _ => "",
        };
        return Err(format!("腾讯云翻译失败（{}）：{}{}", code, message, hint));
    }
    response
        .get("TargetText")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| "腾讯云响应中没有找到译文".to_string())
}

pub fn parse_microsoft(body: &Value) -> Result<String, String> {
    body.get(0)
        .and_then(|item| item.get("translations"))
        .and_then(|t| t.get(0))
        .and_then(|t| t.get("text"))
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| "Microsoft Translator 没有返回译文".to_string())
}

pub fn parse_baidu(body: &Value) -> Result<String, String> {
    if let Some(code) = body.get("error_code").and_then(Value::as_str) {
        let msg = body
            .get("error_msg")
            .and_then(Value::as_str)
            .unwrap_or("未知错误");
        let hint = match code {
            "52003" => "：请检查配置里的 APP ID / 密钥是否正确",
            "54001" => "：签名错误，请检查密钥是否填错",
            "54003" => "：请求太频繁（标准版限 1 次/秒），请稍后再试",
            "54004" => "：账户余额不足或免费额度已用完",
            "58002" => "：翻译服务当前已关闭，请到百度翻译开放平台控制台检查",
            _ => "",
        };
        return Err(format!("百度翻译失败（{}）：{}{}", code, msg, hint));
    }
    body.get("trans_result")
        .and_then(|r| r.get(0))
        .and_then(|t| t.get("dst"))
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| "百度翻译没有返回译文".to_string())
}

/// 解析 Google Apps Script 自建翻译的响应：
/// 成功 {"code":200,"translatedText":"..."}（批量时 translatedText 为数组），
/// 失败 {"code":非200,"error":"原因"}。
pub fn parse_apps_script(body: &Value) -> Result<String, String> {
    let code = body.get("code").and_then(Value::as_i64).unwrap_or(0);
    if code != 200 {
        let message = body
            .get("error")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("Apps Script 接口返回错误");
        return Err(format!("Apps Script 翻译失败（code {}）：{}", code, message));
    }
    match body.get("translatedText") {
        Some(Value::String(s)) if !s.trim().is_empty() => Ok(s.clone()),
        Some(Value::Array(items)) => {
            let parts: Vec<String> = items
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if parts.is_empty() {
                Err("Apps Script 响应里没有译文".to_string())
            } else {
                Ok(parts.join("\n"))
            }
        }
        _ => Err("Apps Script 响应里没有译文".to_string()),
    }
}

pub fn parse_volcano(body: &Value) -> Result<String, String> {
    if let Some(err) = body
        .get("ResponseMetadata")
        .and_then(|m| m.get("Error"))
    {
        let code = err.get("Code").and_then(Value::as_str).unwrap_or("Unknown");
        let message = err.get("Message").and_then(Value::as_str).unwrap_or("未知错误");
        let hint = match code {
            "InvalidAccessKeyId" | "SignatureDoesNotMatch" => {
                "：请检查配置里的 AccessKey / SecretKey 是否正确"
            }
            "InvalidActionOrVersion" => "：接口版本异常，请更新软件",
            "MissingParameter" | "InvalidParameter" => "：请求参数异常",
            "FlowLimitExceeded" | "RequestLimitExceeded" => "：请求太频繁，请稍后再试",
            "ServiceUnavailable" => "：火山服务暂不可用，请稍后再试",
            _ => "",
        };
        return Err(format!("火山引擎翻译失败（{}）：{}{}", code, message, hint));
    }
    body.get("Result")
        .and_then(|r| r.get("TranslationList"))
        .and_then(|l| l.get(0))
        .and_then(|t| t.get("Translation"))
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| "火山引擎没有返回译文".to_string())
}

// ---------- 腾讯云 TMT（TC3-HMAC-SHA256 签名，移植自原 TranslationControl 项目的实现） ----------

mod tencent_sign {
    use hmac::{Hmac, Mac};
    use sha2::{Digest, Sha256};

    type HmacSha256 = Hmac<Sha256>;

    pub fn sha256_hex(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        hex::encode(hasher.finalize())
    }

    pub fn hmac_sha256(key: &[u8], value: &str) -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
        mac.update(value.as_bytes());
        mac.finalize().into_bytes().to_vec()
    }

    /// 生成 Authorization 头（时间戳由调用方传入，便于测试）。
    pub fn authorization(
        secret_id: &str,
        secret_key: &str,
        payload: &str,
        timestamp: i64,
        date: &str,
    ) -> String {
        const HOST: &str = "tmt.tencentcloudapi.com";
        const SERVICE: &str = "tmt";
        let canonical_headers = format!("content-type:application/json\nhost:{}\n", HOST);
        let signed_headers = "content-type;host";
        let canonical_request = format!(
            "POST\n/\n\n{}\n{}\n{}",
            canonical_headers,
            signed_headers,
            sha256_hex(payload)
        );
        let credential_scope = format!("{}/{}/tc3_request", date, SERVICE);
        let string_to_sign = format!(
            "TC3-HMAC-SHA256\n{}\n{}\n{}",
            timestamp,
            credential_scope,
            sha256_hex(&canonical_request)
        );
        let secret_date = hmac_sha256(format!("TC3{}", secret_key).as_bytes(), date);
        let secret_service = hmac_sha256(&secret_date, SERVICE);
        let secret_signing = hmac_sha256(&secret_service, "tc3_request");
        let signature = hex::encode(hmac_sha256(&secret_signing, &string_to_sign));
        format!(
            "TC3-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            secret_id, credential_scope, signed_headers, signature
        )
    }
}

#[async_trait::async_trait]
impl TranslationProvider for TencentProvider {
    fn id(&self) -> &'static str {
        "tencent"
    }
    async fn translate(
        &self,
        client: &reqwest::Client,
        text: &str,
        source: &str,
        target: &str,
        config: &ProviderConfig,
    ) -> Result<String, String> {
        use reqwest::header::{HeaderValue, AUTHORIZATION, CONTENT_TYPE, HOST};
        const TENCENT_HOST: &str = "tmt.tencentcloudapi.com";
        if config.tencent_secret_id.trim().is_empty() || config.tencent_secret_key.trim().is_empty() {
            return Err("尚未配置腾讯云密钥：请打开配置窗口填写 SecretId 和 SecretKey（或切换到其他翻译服务）".to_string());
        }
        let region = if config.tencent_region.trim().is_empty() {
            "ap-guangzhou"
        } else {
            config.tencent_region.trim()
        };
        let now = time::OffsetDateTime::now_utc();
        let timestamp = now.unix_timestamp();
        let date = now.date().to_string();
        let payload = serde_json::to_string(&json!({
            "Source": source,
            "Target": target,
            "SourceText": text,
            "ProjectId": 0,
        }))
        .map_err(|_| "无法生成腾讯云请求".to_string())?;
        let authorization = tencent_sign::authorization(
            config.tencent_secret_id.trim(),
            config.tencent_secret_key.trim(),
            &payload,
            timestamp,
            &date,
        );
        let resp = client
            .post(format!("https://{}", TENCENT_HOST))
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .header(HOST, HeaderValue::from_static(TENCENT_HOST))
            .header("X-TC-Action", HeaderValue::from_static("TextTranslate"))
            .header("X-TC-Version", HeaderValue::from_static("2018-03-21"))
            .header(
                "X-TC-Region",
                HeaderValue::from_str(region).map_err(|_| "腾讯云地域格式无效".to_string())?,
            )
            .header(
                "X-TC-Timestamp",
                HeaderValue::from_str(&timestamp.to_string()).map_err(|_| "时间戳无效".to_string())?,
            )
            .header(
                AUTHORIZATION,
                HeaderValue::from_str(&authorization).map_err(|_| "腾讯云签名生成失败".to_string())?,
            )
            .body(payload)
            .send()
            .await
            .map_err(|e| net_error("腾讯云", &e))?;
        let status = resp.status();
        let body: Value = resp.json().await.map_err(|_| "腾讯云返回了无法解析的响应".to_string())?;
        if !status.is_success() && body.get("Response").and_then(|r| r.get("Error")).is_none() {
            return Err(format!("腾讯云请求失败（HTTP {}）", status.as_u16()));
        }
        parse_tencent(&body)
    }
}

/// MD5（RFC 1321）纯 Rust 实现，供百度翻译签名使用，避免新增依赖。
mod md5 {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
        5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
        4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
        6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee,
        0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
        0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be,
        0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
        0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa,
        0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
        0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
        0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c,
        0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
        0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05,
        0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
        0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039,
        0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1,
        0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
    ];

    pub fn hex(input: &str) -> String {
        compute(input.as_bytes())
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }

    fn compute(input: &[u8]) -> [u8; 16] {
        let mut a0: u32 = 0x67452301;
        let mut b0: u32 = 0xefcdab89;
        let mut c0: u32 = 0x98badcfe;
        let mut d0: u32 = 0x10325476;

        let mut msg = input.to_vec();
        let bit_len = (input.len() as u64).wrapping_mul(8);
        msg.push(0x80);
        while msg.len() % 64 != 56 {
            msg.push(0);
        }
        msg.extend_from_slice(&bit_len.to_le_bytes());

        for chunk in msg.chunks_exact(64) {
            let mut m = [0u32; 16];
            for (i, word) in m.iter_mut().enumerate() {
                *word = u32::from_le_bytes([
                    chunk[i * 4],
                    chunk[i * 4 + 1],
                    chunk[i * 4 + 2],
                    chunk[i * 4 + 3],
                ]);
            }
            let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
            for i in 0..64 {
                let (mut f, g) = match i / 16 {
                    0 => ((b & c) | (!b & d), i),
                    1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                    2 => (b ^ c ^ d, (3 * i + 5) % 16),
                    _ => (c ^ (b | !d), (7 * i) % 16),
                };
                f = f
                    .wrapping_add(a)
                    .wrapping_add(K[i])
                    .wrapping_add(m[g]);
                a = d;
                d = c;
                c = b;
                b = b.wrapping_add(f.rotate_left(S[i]));
            }
            a0 = a0.wrapping_add(a);
            b0 = b0.wrapping_add(b);
            c0 = c0.wrapping_add(c);
            d0 = d0.wrapping_add(d);
        }

        let mut out = [0u8; 16];
        out[0..4].copy_from_slice(&a0.to_le_bytes());
        out[4..8].copy_from_slice(&b0.to_le_bytes());
        out[8..12].copy_from_slice(&c0.to_le_bytes());
        out[12..16].copy_from_slice(&d0.to_le_bytes());
        out
    }
}

// ---------- Google Apps Script 自建翻译 ----------
// 用户自己部署的 Apps Script 网络应用：POST {部署地址/exec}，
// JSON 体 {token, text, source, target}，语言代码用 BCP-47（zh 要写成 zh-CN）。
// script.google.com 会 302 跳转到 googleusercontent 取结果，reqwest 默认跟随跳转。
// 本地网络无法直连 Google 时可在配置里填本地代理（仅该引擎使用独立 client）。

#[async_trait::async_trait]
impl TranslationProvider for AppsScriptProvider {
    fn id(&self) -> &'static str {
        "appsscript"
    }
    async fn translate(
        &self,
        client: &reqwest::Client,
        text: &str,
        source: &str,
        target: &str,
        config: &ProviderConfig,
    ) -> Result<String, String> {
        let url = config.apps_script_url.trim();
        if url.is_empty() {
            return Err("尚未配置 Apps Script 部署地址：请打开配置窗口填写以 /exec 结尾的部署地址（或切换其他翻译服务）".to_string());
        }
        if !url.starts_with("https://script.google.com/") || !url.ends_with("/exec") {
            return Err("Apps Script 地址格式不对：应为 https://script.google.com/macros/s/.../exec".to_string());
        }
        let map_lang = |code: &str| if code == "zh" { "zh-CN".to_string() } else { code.to_string() };
        let payload = json!({
            "token": config.apps_script_token.trim(),
            "text": text,
            "source": map_lang(source),
            "target": map_lang(target),
        });
        // 配置了代理时为该引擎单独建 client，否则用共享 client
        let proxy = config.apps_script_proxy.trim();
        let own_client;
        let active_client: &reqwest::Client = if proxy.is_empty() {
            client
        } else {
            let parsed = reqwest::Proxy::all(proxy)
                .map_err(|_| "代理地址格式不对，应形如 http://127.0.0.1:4602".to_string())?;
            own_client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(config.timeout_secs.clamp(3, 30)))
                .proxy(parsed)
                .build()
                .map_err(|_| "无法按所填代理创建网络客户端".to_string())?;
            &own_client
        };
        let resp = active_client
            .post(url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| net_error("Apps Script", &e))?;
        let status = resp.status();
        let body: Value = resp
            .json()
            .await
            .map_err(|_| format!("Apps Script 返回了无法解析的响应（HTTP {}）", status.as_u16()))?;
        if !status.is_success() && body.get("code").is_none() {
            return Err(format!("Apps Script 请求失败（HTTP {}）", status.as_u16()));
        }
        parse_apps_script(&body)
    }
}

// ---------- 火山引擎机器翻译（SignV4，HMAC-SHA256） ----------

mod volcano_sign {
    use super::tencent_sign::{hmac_sha256, sha256_hex};

    /// 生成 Authorization 头（时间由调用方传入，便于测试）。
    /// datetime 形如 20261106T045900Z，date 形如 20261106。
    pub fn authorization(
        access_key: &str,
        secret_key: &str,
        region: &str,
        payload: &str,
        datetime: &str,
        date: &str,
    ) -> String {
        const HOST: &str = "translate.volcengineapi.com";
        const SERVICE: &str = "translate";
        let payload_hash = sha256_hex(payload);
        let canonical_headers = format!(
            "content-type:application/json\nhost:{}\nx-content-sha256:{}\nx-date:{}\n",
            HOST, payload_hash, datetime
        );
        let signed_headers = "content-type;host;x-content-sha256;x-date";
        let canonical_request = format!(
            "POST\n/\nAction=TranslateText&Version=2020-06-01\n{}\n{}\n{}",
            canonical_headers, signed_headers, payload_hash
        );
        let credential_scope = format!("{}/{}/{}/request", date, region, SERVICE);
        let string_to_sign = format!(
            "HMAC-SHA256\n{}\n{}\n{}",
            datetime,
            credential_scope,
            sha256_hex(&canonical_request)
        );
        let k_date = hmac_sha256(format!("VOLC{}", secret_key).as_bytes(), date);
        let k_region = hmac_sha256(&k_date, region);
        let k_service = hmac_sha256(&k_region, SERVICE);
        let k_signing = hmac_sha256(&k_service, "request");
        let signature = hex::encode(hmac_sha256(&k_signing, &string_to_sign));
        format!(
            "HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            access_key, credential_scope, signed_headers, signature
        )
    }
}

#[async_trait::async_trait]
impl TranslationProvider for GoogleProvider {
    fn id(&self) -> &'static str {
        "google"
    }
    async fn translate(
        &self,
        client: &reqwest::Client,
        text: &str,
        source: &str,
        target: &str,
        _config: &ProviderConfig,
    ) -> Result<String, String> {
        let sl = lang_for("google", source);
        let tl = lang_for("google", target);
        let resp = client
            .get("https://translate.googleapis.com/translate_a/single")
            .query(&[
                ("client", "gtx"),
                ("sl", sl.as_str()),
                ("tl", tl.as_str()),
                ("dt", "t"),
                ("q", text),
            ])
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) EnglishCompanion/1.0")
            .send()
            .await
            .map_err(|e| net_error("Google", &e))?;
        let status = resp.status();
        if status.as_u16() == 429 {
            return Err("Google 免费接口限流（HTTP 429）：这是非官方免费接口、没有稳定性保证，请稍后重试或切换备用服务".to_string());
        }
        if !status.is_success() {
            return Err(format!("Google 接口请求失败（HTTP {}）", status.as_u16()));
        }
        let body: Value = resp.json().await.map_err(|_| "Google 接口返回无法解析".to_string())?;
        parse_google(&body)
    }
}

#[async_trait::async_trait]
impl TranslationProvider for MicrosoftProvider {
    fn id(&self) -> &'static str {
        "microsoft"
    }
    async fn translate(
        &self,
        client: &reqwest::Client,
        text: &str,
        source: &str,
        target: &str,
        config: &ProviderConfig,
    ) -> Result<String, String> {
        if config.microsoft_key.trim().is_empty() {
            return Err("尚未配置 Microsoft Translator 的 API Key，请在设置中填写（Azure 免费额度 F0 每月 200 万字符）".to_string());
        }
        let mut req = client
            .post("https://api.cognitive.microsofttranslator.com/translate")
            .query(&[("api-version", "3.0"), ("from", source), ("to", target)])
            .header("Ocp-Apim-Subscription-Key", config.microsoft_key.trim())
            .json(&json!([{ "Text": text }]));
        if !config.microsoft_region.trim().is_empty() {
            req = req.header("Ocp-Apim-Subscription-Region", config.microsoft_region.trim());
        }
        let resp = req.send().await.map_err(|e| net_error("Microsoft", &e))?;
        let status = resp.status();
        if !status.is_success() {
            let detail = resp.text().await.unwrap_or_default();
            let hint = if status.as_u16() == 401 || status.as_u16() == 403 {
                "请检查 API Key 与区域是否正确"
            } else {
                "请稍后重试"
            };
            return Err(format!(
                "Microsoft Translator 请求失败（HTTP {}）：{}。{}",
                status.as_u16(),
                truncate(&detail, 120),
                hint
            ));
        }
        let body: Value = resp.json().await.map_err(|_| "Microsoft 返回无法解析".to_string())?;
        parse_microsoft(&body)
    }
}

#[async_trait::async_trait]
impl TranslationProvider for MyMemoryProvider {
    fn id(&self) -> &'static str {
        "mymemory"
    }
    async fn translate(
        &self,
        client: &reqwest::Client,
        text: &str,
        source: &str,
        target: &str,
        config: &ProviderConfig,
    ) -> Result<String, String> {
        let langpair = format!("{}|{}", lang_for("mymemory", source), lang_for("mymemory", target));
        let mut query = vec![("q", text.to_string()), ("langpair", langpair)];
        if !config.mymemory_email.trim().is_empty() {
            query.push(("de", config.mymemory_email.trim().to_string()));
        }
        let resp = client
            .get("https://api.mymemory.translated.net/get")
            .query(&query)
            .send()
            .await
            .map_err(|e| net_error("MyMemory", &e))?;
        if resp.status().as_u16() == 429 {
            return Err("MyMemory 免费额度已用完（HTTP 429），请明天再试或切换其他服务".to_string());
        }
        let body: Value = resp.json().await.map_err(|_| "MyMemory 返回无法解析".to_string())?;
        parse_mymemory(&body)
    }
}

#[async_trait::async_trait]
impl TranslationProvider for LibreProvider {
    fn id(&self) -> &'static str {
        "libre"
    }
    async fn translate(
        &self,
        client: &reqwest::Client,
        text: &str,
        source: &str,
        target: &str,
        config: &ProviderConfig,
    ) -> Result<String, String> {
        let base = config.libre_url.trim().trim_end_matches('/');
        if base.is_empty() {
            return Err("尚未配置 LibreTranslate 服务地址，请在设置中填写自建地址".to_string());
        }
        let mut payload = json!({
            "q": text,
            "source": source,
            "target": target,
            "format": "text"
        });
        if !config.libre_key.trim().is_empty() {
            payload["api_key"] = json!(config.libre_key.trim());
        }
        let resp = client
            .post(format!("{}/translate", base))
            .json(&payload)
            .send()
            .await
            .map_err(|e| net_error("LibreTranslate", &e))?;
        let status = resp.status();
        if !status.is_success() {
            let detail = resp.text().await.unwrap_or_default();
            return Err(format!(
                "LibreTranslate 请求失败（HTTP {}）：{}",
                status.as_u16(),
                truncate(&detail, 120)
            ));
        }
        let body: Value = resp.json().await.map_err(|_| "LibreTranslate 返回无法解析".to_string())?;
        parse_libre(&body)
    }
}

#[async_trait::async_trait]
impl TranslationProvider for BaiduProvider {
    fn id(&self) -> &'static str {
        "baidu"
    }
    async fn translate(
        &self,
        client: &reqwest::Client,
        text: &str,
        source: &str,
        target: &str,
        config: &ProviderConfig,
    ) -> Result<String, String> {
        let app_id = config.baidu_app_id.trim();
        let key = config.baidu_key.trim();
        if app_id.is_empty() || key.is_empty() {
            return Err("尚未配置百度翻译的 APP ID / 密钥：请到 fanyi-api.baidu.com 注册后在配置窗口填写（或切换到其他翻译服务）".to_string());
        }
        // 签名 = MD5(appid + q + salt + 密钥)，q 参与签名时不做 URL 编码（百度文档口径）。
        let salt = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis().to_string())
            .unwrap_or_else(|_| "1".to_string());
        let sign = md5::hex(&format!("{}{}{}{}", app_id, text, salt, key));
        let resp = client
            .get("https://fanyi-api.baidu.com/api/trans/vip/translate")
            .query(&[
                ("q", text),
                ("from", source),
                ("to", target),
                ("appid", app_id),
                ("salt", salt.as_str()),
                ("sign", sign.as_str()),
            ])
            .send()
            .await
            .map_err(|e| net_error("百度翻译", &e))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("百度翻译请求失败（HTTP {}）", status.as_u16()));
        }
        let body: Value = resp.json().await.map_err(|_| "百度翻译返回无法解析".to_string())?;
        parse_baidu(&body)
    }
}

#[async_trait::async_trait]
impl TranslationProvider for VolcanoProvider {
    fn id(&self) -> &'static str {
        "volcano"
    }
    async fn translate(
        &self,
        client: &reqwest::Client,
        text: &str,
        source: &str,
        target: &str,
        config: &ProviderConfig,
    ) -> Result<String, String> {
        use reqwest::header::{HeaderValue, AUTHORIZATION, CONTENT_TYPE};
        let access_key = config.volcano_access_key.trim();
        let secret_key = config.volcano_secret_key.trim();
        if access_key.is_empty() || secret_key.is_empty() {
            return Err("尚未配置火山引擎的 AccessKey：请到火山引擎控制台开通机器翻译并创建密钥后在配置窗口填写（或切换到其他翻译服务）".to_string());
        }
        let region = if config.volcano_region.trim().is_empty() {
            "cn-north-1"
        } else {
            config.volcano_region.trim()
        };
        let now = time::OffsetDateTime::now_utc();
        let date = format!("{:04}{:02}{:02}", now.year(), now.month() as u8, now.day());
        let datetime = format!(
            "{}T{:02}{:02}{:02}Z",
            date,
            now.hour(),
            now.minute(),
            now.second()
        );
        let payload = serde_json::to_string(&json!({
            "SourceLanguage": source,
            "TargetLanguage": target,
            "TextList": [text],
        }))
        .map_err(|_| "无法生成火山引擎请求".to_string())?;
        let authorization = volcano_sign::authorization(
            access_key,
            secret_key,
            region,
            &payload,
            &datetime,
            &date,
        );
        let payload_hash = tencent_sign::sha256_hex(&payload);
        let resp = client
            .post("https://translate.volcengineapi.com/?Action=TranslateText&Version=2020-06-01")
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .header(
                "X-Date",
                HeaderValue::from_str(&datetime).map_err(|_| "时间格式无效".to_string())?,
            )
            .header(
                "X-Content-Sha256",
                HeaderValue::from_str(&payload_hash).map_err(|_| "火山引擎签名生成失败".to_string())?,
            )
            .header(
                AUTHORIZATION,
                HeaderValue::from_str(&authorization).map_err(|_| "火山引擎签名生成失败".to_string())?,
            )
            .body(payload)
            .send()
            .await
            .map_err(|e| net_error("火山引擎", &e))?;
        let status = resp.status();
        let body: Value = resp.json().await.map_err(|_| "火山引擎返回了无法解析的响应".to_string())?;
        if !status.is_success()
            && body
                .get("ResponseMetadata")
                .and_then(|m| m.get("Error"))
                .is_none()
        {
            return Err(format!("火山引擎请求失败（HTTP {}）", status.as_u16()));
        }
        parse_volcano(&body)
    }
}

fn net_error(service: &str, e: &reqwest::Error) -> String {
    if e.is_timeout() {
        format!("{} 请求超时，请检查网络后重试", service)
    } else if e.is_connect() {
        format!("无法连接 {}，请检查网络连接", service)
    } else {
        format!("{} 请求失败：{}", service, e)
    }
}

fn truncate(s: &str, max: usize) -> String {
    let t: String = s.chars().take(max).collect();
    t.replace('\n', " ")
}

/// 带重试的单服务调用：网络错误 / 429 / 5xx 时重试 1 次（共 2 次尝试）。
async fn translate_with_retry(
    provider: &dyn TranslationProvider,
    client: &reqwest::Client,
    text: &str,
    source: &str,
    target: &str,
    config: &ProviderConfig,
) -> Result<String, String> {
    let mut last_err = String::new();
    for attempt in 0..2 {
        match provider.translate(client, text, source, target, config).await {
            Ok(v) => return Ok(v),
            Err(e) => {
                let retryable = retry_class(&e) == RetryClass::Retryable;
                last_err = e;
                if attempt == 0 && retryable {
                    tokio::time::sleep(Duration::from_millis(600)).await;
                    continue;
                }
                break;
            }
        }
    }
    Err(last_err)
}

/// 完整翻译流程（单个分块）：主服务 -> 备用服务；技术名词保护/还原在外层统一处理时，
/// 这里接收的已是保护后的文本，返回的也仍含占位符。
pub async fn translate_chunk(
    client: &reqwest::Client,
    text: &str,
    source: &str,
    target: &str,
    config: &ProviderConfig,
) -> Result<(String, String), String> {
    let primary = make_provider(&config.provider)
        .ok_or_else(|| format!("未知的翻译服务：{}", config.provider))?;
    match translate_with_retry(primary.as_ref(), client, text, source, target, config).await {
        Ok(v) => Ok((v, config.provider.clone())),
        Err(primary_err) => {
            let fallback = config.fallback_provider.trim();
            if fallback.is_empty() || fallback == config.provider {
                return Err(primary_err);
            }
            let Some(fb) = make_provider(fallback) else {
                return Err(primary_err);
            };
            match translate_with_retry(fb.as_ref(), client, text, source, target, config).await {
                Ok(v) => Ok((v, fallback.to_string())),
                Err(fb_err) => Err(format!(
                    "{} 失败：{}；备用 {} 也失败：{}",
                    provider_display_name(&config.provider),
                    primary_err,
                    provider_display_name(fallback),
                    fb_err
                )),
            }
        }
    }
}

/// 对外主流程：分块 + 保护技术名词 + 翻译 + 还原。返回 (译文, 实际使用的服务 id)。
pub async fn translate_full(
    text: &str,
    source: &str,
    target: &str,
    config: &ProviderConfig,
) -> Result<(String, String), String> {
    let client = http_client(config.timeout_secs)?;
    // 整次翻译设总截止时间：主引擎重试 + 备用引擎 + 多分块串行也不会无限等下去。
    let total_secs = (config.timeout_secs.clamp(3, 30) * 3).clamp(15, 60);
    let work = async {
        let (protected, used_terms) = terms::protect(text);
        let chunks = terms::chunk_text(&protected, 1200);
        let mut outputs = Vec::new();
        let mut used_provider = config.provider.clone();
        for chunk in &chunks {
            let (translated, provider_used) =
                translate_chunk(&client, chunk, source, target, config).await?;
            used_provider = provider_used;
            outputs.push(translated);
        }
        let joined = if target == "zh" { outputs.join("") } else { outputs.join(" ") };
        Ok::<(String, String), String>((terms::restore(&joined, &used_terms), used_provider))
    };
    tokio::time::timeout(Duration::from_secs(total_secs), work)
        .await
        .map_err(|_| format!("翻译等待超过 {} 秒已停止：网络或翻译服务响应太慢，请稍后重试或切换引擎", total_secs))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_google_segments() {
        let body = json!([[["Hello", "你好", null, null, 10], [" world", "世界", null, null, 10]], null, "zh-CN"]);
        assert_eq!(parse_google(&body).unwrap(), "Hello world");
    }

    #[test]
    fn parses_mymemory_ok_and_quota() {
        let ok = json!({"responseData": {"translatedText": "Hello"}, "responseStatus": 200});
        assert_eq!(parse_mymemory(&ok).unwrap(), "Hello");
        let quota = json!({"responseData": {"translatedText": ""}, "responseStatus": 429, "responseDetails": "QUOTA FINISHED"});
        assert!(parse_mymemory(&quota).unwrap_err().contains("额度"));
    }

    #[test]
    fn parses_libre_and_microsoft() {
        assert_eq!(parse_libre(&json!({"translatedText": "Hi"})).unwrap(), "Hi");
        assert!(parse_libre(&json!({"error": "key required"})).unwrap_err().contains("key required"));
        let ms = json!([{"translations": [{"text": "你好", "to": "zh"}]}]);
        assert_eq!(parse_microsoft(&ms).unwrap(), "你好");
    }

    #[test]
    fn retry_classification() {
        assert_eq!(retry_class("Google 请求超时，请检查网络后重试"), RetryClass::Retryable);
        assert_eq!(retry_class("Google 免费接口限流（HTTP 429）"), RetryClass::Retryable);
        assert_eq!(retry_class("百度翻译请求失败（HTTP 503）"), RetryClass::Retryable);
        assert_eq!(retry_class("尚未配置腾讯云密钥"), RetryClass::Fatal);
        assert_eq!(retry_class("MyMemory 免费额度已用完（HTTP 429）"), RetryClass::Fatal);
        assert_eq!(retry_class("Apps Script 地址格式不对"), RetryClass::Fatal);
    }

    #[test]
    fn default_config_is_tencent_with_google_fallback() {
        let c = ProviderConfig::default();
        assert_eq!(c.provider, "tencent");
        assert_eq!(c.fallback_provider, "google");
        assert_eq!(c.tencent_region, "ap-guangzhou");
    }

    #[test]
    fn parses_apps_script_shapes() {
        let ok = json!({"code": 200, "source": "zh-CN", "target": "en", "translatedText": "System initialization complete"});
        assert_eq!(parse_apps_script(&ok).unwrap(), "System initialization complete");
        let batch = json!({"code": 200, "translatedText": ["Incorrect account or password", "Operation succeeded"]});
        assert_eq!(parse_apps_script(&batch).unwrap(), "Incorrect account or password\nOperation succeeded");
        let err = json!({"code": 401, "error": "invalid token"});
        assert!(parse_apps_script(&err).unwrap_err().contains("invalid token"));
        let empty = json!({"code": 200, "translatedText": ""});
        assert!(parse_apps_script(&empty).is_err());
    }

    #[test]
    fn parses_tencent_ok_and_errors() {
        let ok = json!({"Response": {"TargetText": "Hello", "Source": "zh", "Target": "en"}});
        assert_eq!(parse_tencent(&ok).unwrap(), "Hello");
        let quota = json!({"Response": {"Error": {"Code": "FailedOperation.NoFreeAmount", "Message": "used up"}}});
        assert!(parse_tencent(&quota).unwrap_err().contains("免费额度"));
        let auth = json!({"Response": {"Error": {"Code": "AuthFailure.SignatureFailure", "Message": "bad sign"}}});
        assert!(parse_tencent(&auth).unwrap_err().contains("SecretId"));
    }

    #[test]
    fn tencent_authorization_shape() {
        let auth = super::tencent_sign::authorization("AKIDTEST", "secret", "{}", 1700000000, "2023-11-14");
        assert!(auth.starts_with("TC3-HMAC-SHA256 Credential=AKIDTEST/2023-11-14/tmt/tc3_request, SignedHeaders=content-type;host, Signature="));
        let sig = auth.rsplit("Signature=").next().unwrap();
        assert_eq!(sig.len(), 64);
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// 端到端（本地模拟 LibreTranslate 服务）：验证 HTTP 请求/响应与技术名词还原全链路。
    #[tokio::test]
    async fn libre_end_to_end_with_local_mock() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let body = r#"{"translatedText":"I use ZXTERM0ZX to build APIs"}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        let mut config = ProviderConfig::default();
        config.provider = "libre".to_string();
        config.fallback_provider = String::new();
        config.libre_url = format!("http://127.0.0.1:{}", port);
        let (translated, provider) = translate_full("我用 Spring Boot 写接口", "zh", "en", &config)
            .await
            .unwrap();
        assert_eq!(provider, "libre");
        assert_eq!(translated, "I use Spring Boot to build APIs");
    }

    #[test]
    fn lang_mapping() {
        assert_eq!(lang_for("google", "zh"), "zh-CN");
        assert_eq!(lang_for("libre", "zh"), "zh");
        assert_eq!(lang_for("microsoft", "en"), "en");
    }

    #[test]
    fn md5_vectors() {
        // RFC 1321 附录 A 的测试向量
        assert_eq!(md5::hex(""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5::hex("abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            md5::hex("The quick brown fox jumps over the lazy dog"),
            "9e107d9d372bb6826bd81d3542a419d6"
        );
        assert_eq!(
            md5::hex("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "8215ef0796a20bcaaae116d3876c664a"
        );
        // 中文输入同样稳定输出 32 位小写十六进制
        let zh = md5::hex("你好世界");
        assert_eq!(zh.len(), 32);
        assert!(zh.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn baidu_sign_is_md5_of_appid_q_salt_key() {
        // sign = MD5(appid+q+salt+key)，与百度文档口径一致；此处只验公式形状与确定性
        let a = md5::hex(&format!("{}{}{}{}", "20151231000008489", "apple", "1435660288", "12345678"));
        let b = md5::hex(&format!("{}{}{}{}", "20151231000008489", "apple", "1435660288", "12345678"));
        assert_eq!(a, b);
        assert_eq!(a.len(), 32);
    }

    #[test]
    fn parses_baidu_ok_and_errors() {
        let ok = json!({"from": "zh", "to": "en", "trans_result": [{"src": "你好", "dst": "Hello"}]});
        assert_eq!(parse_baidu(&ok).unwrap(), "Hello");
        let quota = json!({"error_code": "54004", "error_msg": "Invalid Account"});
        assert!(parse_baidu(&quota).unwrap_err().contains("额度"));
        let sign = json!({"error_code": "54001", "error_msg": "Invalid Sign"});
        assert!(parse_baidu(&sign).unwrap_err().contains("签名"));
        let auth = json!({"error_code": "52003", "error_msg": "Unauthorized user"});
        assert!(parse_baidu(&auth).unwrap_err().contains("APP ID"));
    }

    #[test]
    fn parses_volcano_ok_and_errors() {
        let ok = json!({"ResponseMetadata": {"Result": "OK"}, "Result": {"TranslationList": [{"Translation": "Hello", "DetectedSourceLanguage": "zh"}]}});
        assert_eq!(parse_volcano(&ok).unwrap(), "Hello");
        let err = json!({"ResponseMetadata": {"Error": {"Code": "SignatureDoesNotMatch", "Message": "bad sig"}}, "Result": {}});
        assert!(parse_volcano(&err).unwrap_err().contains("AccessKey"));
        let empty = json!({"ResponseMetadata": {"Result": "OK"}, "Result": {"TranslationList": []}});
        assert!(parse_volcano(&empty).is_err());
    }

    #[test]
    fn volcano_authorization_shape() {
        let auth = super::volcano_sign::authorization(
            "AKTEST",
            "secret",
            "cn-north-1",
            "{}",
            "20261106T045900Z",
            "20261106",
        );
        assert!(auth.starts_with("HMAC-SHA256 Credential=AKTEST/20261106/cn-north-1/translate/request, SignedHeaders=content-type;host;x-content-sha256;x-date, Signature="));
        let sig = auth.rsplit("Signature=").next().unwrap();
        assert_eq!(sig.len(), 64);
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn new_providers_resolve_and_have_defaults() {
        assert!(make_provider("baidu").is_some());
        assert!(make_provider("volcano").is_some());
        assert_eq!(make_provider("baidu").unwrap().id(), "baidu");
        assert_eq!(make_provider("volcano").unwrap().id(), "volcano");
        let c = ProviderConfig::default();
        assert_eq!(c.volcano_region, "cn-north-1");
        assert!(c.baidu_app_id.is_empty());
        assert!(c.volcano_access_key.is_empty());
        assert_eq!(provider_display_name("baidu"), "百度翻译");
        assert_eq!(provider_display_name("volcano"), "火山引擎机器翻译");
    }
}
