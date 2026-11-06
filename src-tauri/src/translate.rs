//! 统一翻译引擎：TranslationProvider 接口 + 4 个普通机器翻译服务实现。
//! 不使用任何 AI 大模型接口（无 OpenAI / DeepSeek / Claude 等）。
//!
//! - google    : Google Translate 非官方免费网页接口（client=gtx）。注意：这不是 Google Cloud
//!               官方稳定 API，可能限流或变更，仅作为免费默认引擎，界面与文档均有明确说明。
//! - microsoft : Microsoft Translator 官方 API，需要用户自备 API Key（免费额度 F0 每月 200 万字符）。
//! - mymemory  : MyMemory 免费接口，匿名约 1000 词/天，填邮箱可提升到约 10000 词/天。
//! - libre     : LibreTranslate，用户自建服务地址（默认官方演示站，新版需 Key），开源可自托管。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

use crate::terms;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider: String,          // tencent | google | microsoft | mymemory | libre
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

pub fn make_provider(id: &str) -> Option<Box<dyn TranslationProvider>> {
    match id {
        "tencent" => Some(Box::new(TencentProvider)),
        "google" => Some(Box::new(GoogleProvider)),
        "microsoft" => Some(Box::new(MicrosoftProvider)),
        "mymemory" => Some(Box::new(MyMemoryProvider)),
        "libre" => Some(Box::new(LibreProvider)),
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

// ---------- 各服务实现 ----------

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
                let retryable = e.contains("超时")
                    || e.contains("无法连接")
                    || e.contains("429")
                    || e.contains("HTTP 5");
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
    let timeout = Duration::from_secs(config.timeout_secs.clamp(3, 30));
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|_| "无法创建网络客户端".to_string())?;
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
    Ok((terms::restore(&joined, &used_terms), used_provider))
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
    fn default_config_is_tencent_with_google_fallback() {
        let c = ProviderConfig::default();
        assert_eq!(c.provider, "tencent");
        assert_eq!(c.fallback_provider, "google");
        assert_eq!(c.tencent_region, "ap-guangzhou");
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
}
