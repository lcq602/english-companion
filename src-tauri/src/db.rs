//! SQLite 本地存储：翻译缓存、翻译历史、单词收藏。
//! 数据库文件位于应用数据目录 english-companion.db。
//! 安全约束：默认只保存用户实际翻译过的内容（缓存/历史），不记录原始键盘输入日志。

use rusqlite::{params, Connection};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::sync::Mutex;

pub struct Db(pub Mutex<Connection>);

/// 翻译缓存保留 30 天、最多 2000 条，避免长期无限增长或一直命中过期译文。
pub const CACHE_TTL_SECS: i64 = 30 * 24 * 60 * 60;
pub const CACHE_MAX_ENTRIES: i64 = 2000;

#[derive(Debug, Clone, Serialize)]
pub struct HistoryItem {
    pub id: i64,
    pub source_text: String,
    pub translated_text: String,
    pub source_lang: String,
    pub target_lang: String,
    pub provider: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FavoriteWord {
    pub id: i64,
    pub word: String,
    pub meaning: String,
    pub phonetic: String,
    pub created_at: i64,
}

pub fn cache_key(source_text: &str, source_lang: &str, target_lang: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}|{}|{}", source_lang, target_lang, source_text).as_bytes());
    hex_encode(&hasher.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Db {
    pub fn open(path: &std::path::Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("无法打开本地数据库：{}", e))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS translation_cache (
                cache_key TEXT PRIMARY KEY,
                source_text TEXT NOT NULL,
                source_lang TEXT NOT NULL,
                target_lang TEXT NOT NULL,
                provider TEXT NOT NULL,
                translated_text TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                hit_count INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS translation_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_text TEXT NOT NULL,
                translated_text TEXT NOT NULL,
                source_lang TEXT NOT NULL,
                target_lang TEXT NOT NULL,
                provider TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_history_created ON translation_history(created_at DESC);
            CREATE TABLE IF NOT EXISTS favorite_words (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                word TEXT NOT NULL UNIQUE,
                meaning TEXT NOT NULL DEFAULT '',
                phonetic TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL
            );",
        )
        .map_err(|e| format!("无法初始化本地数据库：{}", e))?;
        // 旧库迁移：translation_cache 增加 expires_at（旧数据默认 0，一律视为过期）。
        let has_expires: bool = conn
            .prepare("SELECT expires_at FROM translation_cache LIMIT 0")
            .is_ok();
        if !has_expires {
            conn.execute_batch("ALTER TABLE translation_cache ADD COLUMN expires_at INTEGER NOT NULL DEFAULT 0;")
                .map_err(|e| format!("无法升级本地数据库：{}", e))?;
        }
        Ok(Self(Mutex::new(conn)))
    }

    pub fn cache_get(&self, key: &str) -> Option<(String, String)> {
        let conn = self.0.lock().ok()?;
        let mut stmt = conn
            .prepare(
                "SELECT translated_text, provider FROM translation_cache \
                 WHERE cache_key = ?1 AND expires_at > ?2",
            )
            .ok()?;
        let row = stmt
            .query_row(params![key, now_secs()], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .ok()?;
        drop(stmt);
        let _ = conn.execute(
            "UPDATE translation_cache SET hit_count = hit_count + 1 WHERE cache_key = ?1",
            params![key],
        );
        Some(row)
    }

    pub fn cache_put(
        &self,
        key: &str,
        source_text: &str,
        source_lang: &str,
        target_lang: &str,
        provider: &str,
        translated_text: &str,
    ) {
        if let Ok(conn) = self.0.lock() {
            let now = now_secs();
            let _ = conn.execute(
                "INSERT OR REPLACE INTO translation_cache
                 (cache_key, source_text, source_lang, target_lang, provider, translated_text, created_at, hit_count, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
                params![key, source_text, source_lang, target_lang, provider, translated_text, now, now + CACHE_TTL_SECS],
            );
            // 清理过期项，并把总量压到上限以内（保留最新写入的）
            let _ = conn.execute(
                "DELETE FROM translation_cache WHERE expires_at <= ?1",
                params![now],
            );
            let _ = conn.execute(
                "DELETE FROM translation_cache WHERE cache_key NOT IN \
                 (SELECT cache_key FROM translation_cache ORDER BY created_at DESC LIMIT ?1)",
                params![CACHE_MAX_ENTRIES],
            );
        }
    }

    pub fn cache_clear(&self) {
        if let Ok(conn) = self.0.lock() {
            let _ = conn.execute("DELETE FROM translation_cache", []);
        }
    }

    pub fn history_add(
        &self,
        source_text: &str,
        translated_text: &str,
        source_lang: &str,
        target_lang: &str,
        provider: &str,
    ) {
        if let Ok(conn) = self.0.lock() {
            // 与上一条完全相同则不重复记录（连续输入去重）
            let last: Option<String> = conn
                .query_row(
                    "SELECT source_text || '|' || target_lang FROM translation_history ORDER BY id DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .ok();
            if last.as_deref() == Some(&format!("{}|{}", source_text, target_lang)) {
                return;
            }
            let _ = conn.execute(
                "INSERT INTO translation_history (source_text, translated_text, source_lang, target_lang, provider, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![source_text, translated_text, source_lang, target_lang, provider, now_secs()],
            );
            // 历史最多保留 500 条
            let _ = conn.execute(
                "DELETE FROM translation_history WHERE id NOT IN (SELECT id FROM translation_history ORDER BY id DESC LIMIT 500)",
                [],
            );
        }
    }

    pub fn history_list(&self, limit: i64) -> Vec<HistoryItem> {
        let Ok(conn) = self.0.lock() else { return Vec::new() };
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, source_text, translated_text, source_lang, target_lang, provider, created_at
             FROM translation_history ORDER BY id DESC LIMIT ?1",
        ) else { return Vec::new() };
        let rows = stmt.query_map(params![limit], |r| {
            Ok(HistoryItem {
                id: r.get(0)?,
                source_text: r.get(1)?,
                translated_text: r.get(2)?,
                source_lang: r.get(3)?,
                target_lang: r.get(4)?,
                provider: r.get(5)?,
                created_at: r.get(6)?,
            })
        });
        match rows {
            Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn history_delete(&self, id: i64) {
        if let Ok(conn) = self.0.lock() {
            let _ = conn.execute("DELETE FROM translation_history WHERE id = ?1", params![id]);
        }
    }

    pub fn history_clear(&self) {
        if let Ok(conn) = self.0.lock() {
            let _ = conn.execute("DELETE FROM translation_history", []);
        }
    }

    pub fn favorite_add(&self, word: &str, meaning: &str, phonetic: &str) {
        if let Ok(conn) = self.0.lock() {
            let _ = conn.execute(
                "INSERT INTO favorite_words (word, meaning, phonetic, created_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(word) DO UPDATE SET meaning = excluded.meaning, phonetic = excluded.phonetic",
                params![word.to_lowercase(), meaning, phonetic, now_secs()],
            );
        }
    }

    pub fn favorite_remove(&self, word: &str) {
        if let Ok(conn) = self.0.lock() {
            let _ = conn.execute(
                "DELETE FROM favorite_words WHERE word = ?1",
                params![word.to_lowercase()],
            );
        }
    }

    pub fn favorite_list(&self) -> Vec<FavoriteWord> {
        let Ok(conn) = self.0.lock() else { return Vec::new() };
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, word, meaning, phonetic, created_at FROM favorite_words ORDER BY id DESC",
        ) else { return Vec::new() };
        let rows = stmt.query_map([], |r| {
            Ok(FavoriteWord {
                id: r.get(0)?,
                word: r.get(1)?,
                meaning: r.get(2)?,
                phonetic: r.get(3)?,
                created_at: r.get(4)?,
            })
        });
        match rows {
            Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn cache_stats(&self) -> (i64, i64) {
        let Ok(conn) = self.0.lock() else { return (0, 0) };
        let cache_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM translation_cache", [], |r| r.get(0))
            .unwrap_or(0);
        let history_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM translation_history", [], |r| r.get(0))
            .unwrap_or(0);
        (cache_count, history_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Db {
        Db::open(std::path::Path::new(":memory:")).unwrap()
    }

    #[test]
    fn cache_roundtrip() {
        let db = test_db();
        let key = cache_key("你好", "zh", "en");
        assert!(db.cache_get(&key).is_none());
        db.cache_put(&key, "你好", "zh", "en", "google", "Hello");
        let (text, provider) = db.cache_get(&key).unwrap();
        assert_eq!(text, "Hello");
        assert_eq!(provider, "google");
    }

    #[test]
    fn cache_expires_and_clears() {
        let db = test_db();
        let key = cache_key("你好", "zh", "en");
        db.cache_put(&key, "你好", "zh", "en", "google", "Hello");
        assert!(db.cache_get(&key).is_some());
        // 人工把过期时间拨到过去：应视为未命中
        db.0.lock().unwrap()
            .execute("UPDATE translation_cache SET expires_at = 1 WHERE cache_key = ?1", params![key])
            .unwrap();
        assert!(db.cache_get(&key).is_none());
        db.cache_put(&key, "你好", "zh", "en", "google", "Hello");
        db.cache_clear();
        assert!(db.cache_get(&key).is_none());
    }

    #[test]
    fn cache_key_differs_by_direction() {
        assert_ne!(cache_key("你好", "zh", "en"), cache_key("你好", "en", "zh"));
    }

    #[test]
    fn history_dedupes_consecutive() {
        let db = test_db();
        db.history_add("你好", "Hello", "zh", "en", "google");
        db.history_add("你好", "Hello", "zh", "en", "google");
        db.history_add("谢谢", "Thanks", "zh", "en", "google");
        assert_eq!(db.history_list(10).len(), 2);
    }

    #[test]
    fn favorites_upsert_and_remove() {
        let db = test_db();
        db.favorite_add("Hello", "你好", "/həˈləʊ/");
        db.favorite_add("hello", "您好", "");
        let list = db.favorite_list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].meaning, "您好");
        db.favorite_remove("HELLO");
        assert!(db.favorite_list().is_empty());
    }
}
