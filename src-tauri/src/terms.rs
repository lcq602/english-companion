//! 技术名词保护：翻译前把 Java / Spring Boot / Vue 等技术名词替换成占位符，
//! 翻译后再还原，避免机器翻译把专有名词译错或译掉。
//! 占位符形如 ZXTERM0ZX（纯字母数字，机翻通常会原样保留；还原时大小写不敏感匹配）。

pub const TECH_TERMS: &[&str] = &[
    // 优先长词，避免 "Spring" 先于 "Spring Boot" 被替换
    "Spring Boot", "Spring Cloud", "Spring MVC", "Visual Studio Code", "VS Code",
    "TypeScript", "JavaScript", "PostgreSQL", "MySQL", "SQLite", "Redis", "Docker",
    "Kubernetes", "Python", "Vue", "React", "Angular", "Tauri", "Rust", "Java",
    "Spring", "MyBatis", "Maven", "Gradle", "GitHub", "GitLab", "Git", "Linux",
    "Windows", "Android", "iOS", "HTTP", "HTTPS", "JSON", "API", "SDK", "REST",
    "Node.js", "Vite", "Pinia", "Element Plus", "Nginx", "Kafka", "MongoDB",
];

fn placeholder(index: usize) -> String {
    format!("ZXTERM{}ZX", index)
}

/// 返回 (替换后的文本, 实际命中的名词列表)
pub fn protect(text: &str) -> (String, Vec<String>) {
    let mut result = text.to_string();
    let mut used: Vec<String> = Vec::new();
    // 按长度降序，保证长词优先
    let mut terms: Vec<&str> = TECH_TERMS.to_vec();
    terms.sort_by_key(|t| std::cmp::Reverse(t.len()));
    for term in terms {
        if term.is_ascii() {
            // 英文名词：大小写不敏感查找（输入里多为原样英文）
            let lower_result = result.to_lowercase();
            let lower_term = term.to_lowercase();
            if lower_result.len() != result.len() {
                // 极少数 Unicode 字符（如 İ）小写化会改变字节长度，
                // 此时用小写串的下标切原文会错位甚至 panic，退回精确匹配替换。
                if result.contains(term) {
                    let idx = used.len();
                    result = result.replace(term, &placeholder(idx));
                    used.push(term.to_string());
                }
                continue;
            }
            if lower_result.contains(&lower_term) {
                let idx = used.len();
                // 简单逐个替换（不区分大小写，用小写定位后按原长度切分）
                let mut rebuilt = String::new();
                let mut hay = result.clone();
                let mut hay_lower = lower_result.clone();
                while let Some(pos) = hay_lower.find(&lower_term) {
                    rebuilt.push_str(&hay[..pos]);
                    rebuilt.push_str(&placeholder(idx));
                    hay = hay[pos + term.len()..].to_string();
                    hay_lower = hay_lower[pos + lower_term.len()..].to_string();
                }
                rebuilt.push_str(&hay);
                result = rebuilt;
                used.push(term.to_string());
            }
        }
    }
    (result, used)
}

/// 把译文中的占位符还原为技术名词（大小写不敏感，兼容机翻把占位符改成小写的情况）。
pub fn restore(text: &str, used: &[String]) -> String {
    let mut result = text.to_string();
    for (i, term) in used.iter().enumerate() {
        let ph = placeholder(i);
        // 原样替换
        result = result.replace(&ph, term);
        // 小写形式替换
        result = result.replace(&ph.to_lowercase(), term);
        // 机翻可能在占位符中间插入空格：ZX TERM 0 ZX -> 做一次宽松处理
        let spaced = ph.chars().map(|c| c.to_string()).collect::<Vec<_>>().join(" ");
        result = result.replace(&spaced, term);
    }
    result
}

/// 长文本分块：按句末标点切分，每块不超过 max_chars，避免 GET URL 过长 / 单请求超限。
pub fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    let text = text.trim();
    if text.chars().count() <= max_chars {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;
    for ch in text.chars() {
        current.push(ch);
        current_len += 1;
        let is_break = matches!(ch, '。' | '！' | '？' | '.' | '!' | '?' | '\n' | '；' | ';');
        if is_break && current_len >= max_chars / 2 || current_len >= max_chars {
            chunks.push(current.trim().to_string());
            current = String::new();
            current_len = 0;
        }
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    chunks.into_iter().filter(|c| !c.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protects_spring_boot_before_spring() {
        let (protected, used) = protect("我用 Spring Boot 和 Vue 写接口");
        assert!(used.contains(&"Spring Boot".to_string()));
        assert!(used.contains(&"Vue".to_string()));
        assert!(!protected.contains("Spring Boot"));
        let restored = restore("I use ZXTERM0ZX and ZXTERM1ZX", &used);
        // 还原顺序与 used 顺序一致
        assert!(restored.contains("Spring Boot") || restored.contains("Vue"));
    }

    #[test]
    fn protect_survives_case_expanding_unicode() {
        // İ 小写化会改变字节长度：不能 panic，精确命中的名词仍应被保护
        let (protected, used) = protect("İ Spring Boot");
        assert!(used.contains(&"Spring Boot".to_string()));
        assert!(!protected.contains("Spring Boot"));
    }

    #[test]
    fn restore_is_case_insensitive() {
        let used = vec!["Python".to_string()];
        assert_eq!(restore("i like zxterm0zx", &used), "i like Python");
    }

    #[test]
    fn no_terms_no_change() {
        let (protected, used) = protect("今天天气很好");
        assert_eq!(protected, "今天天气很好");
        assert!(used.is_empty());
    }

    #[test]
    fn chunks_long_text() {
        let text = "第一句话很长很长。第二句话也很长很长！第三句？".repeat(30);
        let chunks = chunk_text(&text, 100);
        assert!(chunks.len() > 1);
        for c in &chunks {
            assert!(c.chars().count() <= 101, "chunk too long: {}", c.chars().count());
        }
    }

    #[test]
    fn short_text_single_chunk() {
        assert_eq!(chunk_text("你好", 100), vec!["你好".to_string()]);
    }
}
