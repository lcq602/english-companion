//! 语言检测：与原项目 src/lib/language.ts 的 detectDirection 逻辑保持一致，
//! 移植到 Rust 端，供翻译编排（自动识别源语言、决定目标语言）使用。

/// 检测结果：源文本主要是中文 / 英文 / 混合 / 空。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detected {
    Chinese,
    English,
    Mixed,
    Empty,
}

pub fn detect(text: &str) -> Detected {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Detected::Empty;
    }
    let cjk = trimmed
        .chars()
        .filter(|c| ('\u{3400}'..='\u{4dbf}').contains(c) || ('\u{4e00}'..='\u{9fff}').contains(c))
        .count() as f64;
    let latin = trimmed.chars().filter(|c| c.is_ascii_alphabetic()).count() as f64;
    if cjk == 0.0 && latin > 0.0 {
        return Detected::English;
    }
    if latin == 0.0 && cjk > 0.0 {
        return Detected::Chinese;
    }
    if cjk > 0.0 && latin > 0.0 {
        let ratio = cjk / latin;
        if ratio > 0.25 && ratio < 4.0 {
            return Detected::Mixed;
        }
    }
    if cjk > latin * 0.35 {
        return Detected::Chinese;
    }
    if latin > cjk * 0.35 {
        return Detected::English;
    }
    Detected::Mixed
}

/// 把前端传入的 source 配置（auto/zh/en）解析为实际源语言代码。
/// 返回 "zh" 或 "en"。
pub fn resolve_source(text: &str, requested: &str) -> String {
    match requested {
        "zh" | "zh-CN" => "zh".to_string(),
        "en" => "en".to_string(),
        _ => match detect(text) {
            Detected::English => "en".to_string(),
            _ => "zh".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_chinese() {
        assert_eq!(detect("我正在学习英语"), Detected::Chinese);
    }
    #[test]
    fn detects_english() {
        assert_eq!(detect("I am learning English"), Detected::English);
    }
    #[test]
    fn detects_empty() {
        assert_eq!(detect("  \n "), Detected::Empty);
    }
    #[test]
    fn resolve_auto() {
        assert_eq!(resolve_source("你好", "auto"), "zh");
        assert_eq!(resolve_source("hello", "auto"), "en");
        assert_eq!(resolve_source("hello", "zh"), "zh");
    }
}
