//! AppSettings (ai / gemini / git / directory names) persisted as `app-settings.json` in the app config dir.
//! Only the user's overrides are stored; defaults are merged at read time (port of server/utils/settings.ts).

use super::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;

pub const APP_SETTINGS_FILE: &str = "app-settings.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AiSettings {
    pub model: String,
    pub max_tokens: u32,
    pub extra_instructions: String,
    pub command: String,
    pub args: Vec<String>,
    pub timeout_ms: u64,
}

impl Default for AiSettings {
    fn default() -> Self {
        AiSettings {
            model: "claude-sonnet-5-5".into(),
            max_tokens: 8192,
            extra_instructions: String::new(),
            command: "claude".into(),
            args: vec![],
            timeout_ms: 600_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResearchEngine {
    #[serde(rename = "antigravity")]
    Antigravity,
    #[serde(rename = "gemini-cli")]
    GeminiCli,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GeminiSettings {
    pub engine: ResearchEngine,
    pub command: String,
    pub args: Vec<String>,
    pub model: String,
    pub timeout_ms: u64,
    pub cache_hours: f64,
}

impl Default for GeminiSettings {
    fn default() -> Self {
        GeminiSettings {
            engine: ResearchEngine::Antigravity,
            command: "agy".into(),
            args: vec![],
            model: String::new(),
            timeout_ms: 180_000,
            cache_hours: 24.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GitSettings {
    pub remote: String,
    pub branch: String,
    pub author_name: String,
    pub author_email: String,
    pub auto_commit: bool,
    pub auto_push: bool,
}

impl Default for GitSettings {
    fn default() -> Self {
        GitSettings {
            remote: "origin".into(),
            branch: String::new(),
            author_name: String::new(),
            author_email: String::new(),
            auto_commit: false,
            auto_push: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub sources_dir: String,
    pub data_dir: String,
    pub ai: AiSettings,
    pub gemini: GeminiSettings,
    pub git: GitSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            sources_dir: "sources".into(),
            data_dir: "data".into(),
            ai: AiSettings::default(),
            gemini: GeminiSettings::default(),
            git: GitSettings::default(),
        }
    }
}

pub fn deep_merge(base: &Value, patch: &Value) -> Value {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            let mut out = b.clone();
            for (k, v) in p {
                if v.is_null() {
                    continue;
                }
                let merged = match out.get(k) {
                    Some(existing) if existing.is_object() && v.is_object() => deep_merge(existing, v),
                    _ => v.clone(),
                };
                out.insert(k.clone(), merged);
            }
            Value::Object(out)
        }
        (_, p) => p.clone(),
    }
}

/// Older stored overrides: `ai.anthropicModel` (API-key era) became `ai.model`. Keep the user's model, drop the dead key.
pub fn migrate_stored(stored: &mut Value) {
    let Some(ai) = stored.get_mut("ai").and_then(Value::as_object_mut) else { return };
    if let Some(old) = ai.remove("anthropicModel") {
        let has_model = ai.get("model").and_then(Value::as_str).is_some_and(|s| !s.is_empty());
        if !has_model {
            if let Some(m) = old.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                ai.insert("model".into(), json!(m));
            }
        }
    }
}

pub fn effective(overrides: &Value) -> AppSettings {
    let base = serde_json::to_value(AppSettings::default()).unwrap_or(Value::Null);
    let merged = deep_merge(&base, overrides);
    // A malformed stored value (wrong type) must not take the app down: fall back to defaults for the whole object.
    serde_json::from_value(merged).unwrap_or_default()
}

/// Load stored overrides. Missing file -> `{}`. Corrupt / unreadable file -> `{}` plus a warning.
pub fn load_overrides(config_dir: &Path) -> (Value, Option<String>) {
    let file = config_dir.join(APP_SETTINGS_FILE);
    match fs::read_to_string(&file) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(mut v) if v.is_object() => {
                migrate_stored(&mut v);
                (v, None)
            }
            Ok(_) => (json!({}), Some(format!("{} の形式が不正です（既定値を使用）", file.display()))),
            Err(e) => (json!({}), Some(format!("{} を読めません（既定値を使用）: {e}", file.display()))),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (json!({}), None),
        Err(e) => (json!({}), Some(format!("{} を開けません（既定値を使用）: {e}", file.display()))),
    }
}

pub fn save_overrides(config_dir: &Path, overrides: &Value) -> Result<()> {
    fs::create_dir_all(config_dir).map_err(|e| AppError::io("設定フォルダを作れません", e))?;
    let file = config_dir.join(APP_SETTINGS_FILE);
    let tmp = config_dir.join(format!("{APP_SETTINGS_FILE}.tmp"));
    let text = serde_json::to_string_pretty(overrides)?;
    fs::write(&tmp, text).map_err(|e| AppError::io("設定を書き込めません", e))?;
    fs::rename(&tmp, &file).map_err(|e| AppError::io("設定を保存できません", e))
}

/// Git ref / remote names that are safe on a command line (no option injection).
pub fn is_safe_git_name(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    let len = name.chars().count();
    if len > 200 || name.starts_with('-') || name.contains("..") || name.ends_with('/') || name.ends_with(".lock") {
        return false;
    }
    !name.chars().any(|c| c.is_whitespace() || matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\') || (c as u32) < 0x21 || c == '\u{7f}')
}

fn bad(path: &str, why: &str) -> AppError {
    AppError::invalid(format!("設定の値が不正です: {path}: {why}"))
}

fn as_obj<'a>(v: &'a Value, path: &str) -> Result<&'a Map<String, Value>> {
    v.as_object().ok_or_else(|| bad(path, "オブジェクトではありません"))
}

fn check_keys(m: &Map<String, Value>, allowed: &[&str], path: &str) -> Result<()> {
    for k in m.keys() {
        if !allowed.contains(&k.as_str()) {
            let p = if path.is_empty() { k.clone() } else { format!("{path}.{k}") };
            return Err(bad(&p, "未知のキーです"));
        }
    }
    Ok(())
}

fn string_field(m: &Map<String, Value>, key: &str, path: &str, min: usize, max: usize) -> Result<Option<Value>> {
    let Some(v) = m.get(key) else { return Ok(None) };
    let p = format!("{path}{key}");
    let s = v.as_str().ok_or_else(|| bad(&p, "文字列ではありません"))?.trim().to_string();
    let n = s.chars().count();
    if n < min {
        return Err(bad(&p, "空にできません"));
    }
    if n > max {
        return Err(bad(&p, &format!("{max} 文字以内にしてください")));
    }
    Ok(Some(json!(s)))
}

fn raw_string_field(m: &Map<String, Value>, key: &str, path: &str, max: usize) -> Result<Option<Value>> {
    let Some(v) = m.get(key) else { return Ok(None) };
    let p = format!("{path}{key}");
    let s = v.as_str().ok_or_else(|| bad(&p, "文字列ではありません"))?;
    if s.chars().count() > max {
        return Err(bad(&p, &format!("{max} 文字以内にしてください")));
    }
    Ok(Some(json!(s)))
}

fn int_field(m: &Map<String, Value>, key: &str, path: &str, min: i64, max: i64) -> Result<Option<Value>> {
    let Some(v) = m.get(key) else { return Ok(None) };
    let p = format!("{path}{key}");
    let n = v.as_i64().ok_or_else(|| bad(&p, "整数ではありません"))?;
    if n < min || n > max {
        return Err(bad(&p, &format!("{min}〜{max} の範囲で指定してください")));
    }
    Ok(Some(json!(n)))
}

fn bool_field(m: &Map<String, Value>, key: &str, path: &str) -> Result<Option<Value>> {
    let Some(v) = m.get(key) else { return Ok(None) };
    v.as_bool().map(|b| Some(json!(b))).ok_or_else(|| bad(&format!("{path}{key}"), "真偽値ではありません"))
}

fn args_field(m: &Map<String, Value>, path: &str) -> Result<Option<Value>> {
    let Some(v) = m.get("args") else { return Ok(None) };
    let p = format!("{path}args");
    let a = v.as_array().ok_or_else(|| bad(&p, "配列ではありません"))?;
    if a.len() > 50 {
        return Err(bad(&p, "50 個以内にしてください"));
    }
    for x in a {
        let s = x.as_str().ok_or_else(|| bad(&p, "文字列の配列ではありません"))?;
        if s.chars().count() > 1000 {
            return Err(bad(&p, "各要素は 1000 文字以内にしてください"));
        }
    }
    Ok(Some(v.clone()))
}

fn git_name_field(m: &Map<String, Value>, key: &str, path: &str) -> Result<Option<Value>> {
    let v = string_field(m, key, path, 0, 200)?;
    if let Some(Value::String(s)) = &v {
        if !is_safe_git_name(s) {
            return Err(bad(&format!("{path}{key}"), "git の名前として使えない文字が含まれています"));
        }
    }
    Ok(v)
}

fn put(out: &mut Map<String, Value>, key: &str, v: Option<Value>) {
    if let Some(v) = v {
        out.insert(key.into(), v);
    }
}

/// Validate a settings patch (same limits as the old zod schema; unknown keys are rejected) and return the normalised patch.
pub fn validate_patch(patch: &Value) -> Result<Value> {
    let root = as_obj(patch, "(root)")?;
    check_keys(root, &["sourcesDir", "dataDir", "ai", "gemini", "git"], "")?;
    let mut out = Map::new();
    put(&mut out, "sourcesDir", string_field(root, "sourcesDir", "", 1, 1000)?);
    put(&mut out, "dataDir", string_field(root, "dataDir", "", 1, 1000)?);
    if let Some(ai) = root.get("ai") {
        let m = as_obj(ai, "ai")?;
        check_keys(m, &["model", "maxTokens", "extraInstructions", "command", "args", "timeoutMs"], "ai")?;
        let mut o = Map::new();
        put(&mut o, "model", string_field(m, "model", "ai.", 1, 100)?);
        put(&mut o, "maxTokens", int_field(m, "maxTokens", "ai.", 256, 128_000)?);
        put(&mut o, "extraInstructions", raw_string_field(m, "extraInstructions", "ai.", 20_000)?);
        put(&mut o, "command", string_field(m, "command", "ai.", 1, 1000)?);
        put(&mut o, "args", args_field(m, "ai.")?);
        put(&mut o, "timeoutMs", int_field(m, "timeoutMs", "ai.", 1000, 3_600_000)?);
        out.insert("ai".into(), Value::Object(o));
    }
    if let Some(g) = root.get("gemini") {
        let m = as_obj(g, "gemini")?;
        check_keys(m, &["engine", "command", "args", "model", "timeoutMs", "cacheHours"], "gemini")?;
        let mut o = Map::new();
        if let Some(e) = m.get("engine") {
            match e.as_str() {
                Some("antigravity") | Some("gemini-cli") => {
                    o.insert("engine".into(), e.clone());
                }
                _ => return Err(bad("gemini.engine", "antigravity か gemini-cli を指定してください")),
            }
        }
        put(&mut o, "command", string_field(m, "command", "gemini.", 1, 1000)?);
        put(&mut o, "args", args_field(m, "gemini.")?);
        put(&mut o, "model", string_field(m, "model", "gemini.", 0, 100)?);
        put(&mut o, "timeoutMs", int_field(m, "timeoutMs", "gemini.", 1000, 3_600_000)?);
        if let Some(v) = m.get("cacheHours") {
            let n = v.as_f64().ok_or_else(|| bad("gemini.cacheHours", "数値ではありません"))?;
            if !(0.0..=(24.0 * 365.0)).contains(&n) {
                return Err(bad("gemini.cacheHours", "0〜8760 の範囲で指定してください"));
            }
            o.insert("cacheHours".into(), json!(n));
        }
        out.insert("gemini".into(), Value::Object(o));
    }
    if let Some(g) = root.get("git") {
        let m = as_obj(g, "git")?;
        check_keys(m, &["remote", "branch", "authorName", "authorEmail", "autoCommit", "autoPush"], "git")?;
        let mut o = Map::new();
        put(&mut o, "remote", git_name_field(m, "remote", "git.")?);
        put(&mut o, "branch", git_name_field(m, "branch", "git.")?);
        put(&mut o, "authorName", string_field(m, "authorName", "git.", 0, 200)?);
        put(&mut o, "authorEmail", string_field(m, "authorEmail", "git.", 0, 200)?);
        put(&mut o, "autoCommit", bool_field(m, "autoCommit", "git.")?);
        put(&mut o, "autoPush", bool_field(m, "autoPush", "git.")?);
        out.insert("git".into(), Value::Object(o));
    }
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_ts_defaults() {
        let s = effective(&json!({}));
        assert_eq!(s, AppSettings::default());
        assert_eq!(s.sources_dir, "sources");
        assert_eq!(s.data_dir, "data");
        assert_eq!(s.ai.command, "claude");
        assert_eq!(s.ai.model, "claude-sonnet-5-5");
        assert_eq!(s.ai.max_tokens, 8192);
        assert_eq!(s.ai.timeout_ms, 600_000);
        assert_eq!(s.gemini.engine, ResearchEngine::Antigravity);
        assert_eq!(s.gemini.command, "agy");
        assert_eq!(s.gemini.timeout_ms, 180_000);
        assert_eq!(s.gemini.cache_hours, 24.0);
        assert_eq!(s.git.remote, "origin");
        assert!(!s.git.auto_commit);
        let wire = serde_json::to_value(&s).unwrap();
        assert_eq!(wire["gemini"]["engine"], "antigravity");
        assert_eq!(wire["ai"]["maxTokens"], 8192);
    }

    #[test]
    fn overrides_merge_deeply() {
        let s = effective(&json!({ "ai": { "model": "x" }, "git": { "autoCommit": true } }));
        assert_eq!(s.ai.model, "x");
        assert_eq!(s.ai.command, "claude");
        assert!(s.git.auto_commit);
        assert_eq!(s.git.remote, "origin");
    }

    #[test]
    fn legacy_anthropic_model_migrates() {
        let mut v = json!({ "ai": { "anthropicModel": " claude-old " } });
        migrate_stored(&mut v);
        assert_eq!(v["ai"]["model"], "claude-old");
        assert!(v["ai"].get("anthropicModel").is_none());
        // an explicit model wins, and the dead key is still dropped
        let mut v = json!({ "ai": { "anthropicModel": "old", "model": "new" } });
        migrate_stored(&mut v);
        assert_eq!(v["ai"]["model"], "new");
        assert!(v["ai"].get("anthropicModel").is_none());
    }

    #[test]
    fn persists_and_reloads_with_migration() {
        let d = tempfile::tempdir().unwrap();
        let (v, w) = load_overrides(d.path());
        assert_eq!(v, json!({}));
        assert!(w.is_none());
        fs::write(d.path().join(APP_SETTINGS_FILE), r#"{"ai":{"anthropicModel":"m1"},"git":{"remote":"up"}}"#).unwrap();
        let (v, _) = load_overrides(d.path());
        let s = effective(&v);
        assert_eq!(s.ai.model, "m1");
        assert_eq!(s.git.remote, "up");
        save_overrides(d.path(), &v).unwrap();
        assert!(!d.path().join("app-settings.json.tmp").exists());
        let (v2, _) = load_overrides(d.path());
        assert_eq!(v, v2);
    }

    #[test]
    fn corrupt_file_falls_back_with_warning() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join(APP_SETTINGS_FILE), "{ nope").unwrap();
        let (v, w) = load_overrides(d.path());
        assert_eq!(v, json!({}));
        assert!(w.is_some());
    }

    #[test]
    fn wrong_typed_override_falls_back_to_defaults() {
        assert_eq!(effective(&json!({ "ai": { "maxTokens": "lots" } })), AppSettings::default());
    }

    #[test]
    fn patch_validation() {
        let ok = validate_patch(&json!({ "ai": { "model": "  m  ", "maxTokens": 1000 }, "gemini": { "engine": "gemini-cli", "cacheHours": 0 } })).unwrap();
        assert_eq!(ok["ai"]["model"], "m");
        for (bad_patch, needle) in [
            (json!({ "ai": { "maxTokens": 10 } }), "ai.maxTokens"),
            (json!({ "ai": { "model": "" } }), "ai.model"),
            (json!({ "ai": { "nope": 1 } }), "ai.nope"),
            (json!({ "nope": 1 }), "nope"),
            (json!({ "gemini": { "engine": "other" } }), "gemini.engine"),
            (json!({ "git": { "remote": "--upload-pack=x" } }), "git.remote"),
            (json!({ "git": { "branch": "a b" } }), "git.branch"),
            (json!({ "git": { "autoCommit": "yes" } }), "git.autoCommit"),
            (json!({ "gemini": { "args": ["a", 1] } }), "gemini.args"),
        ] {
            let e = validate_patch(&bad_patch).unwrap_err();
            assert_eq!(e.code, super::super::error::ErrorCode::InvalidArgument);
            assert!(e.message.contains(needle), "{} !~ {needle}", e.message);
        }
        assert!(validate_patch(&json!({ "git": { "branch": "" } })).is_ok());
    }

    #[test]
    fn git_name_safety() {
        for ok in ["", "origin", "main", "feat/x-1", "ブランチ"] {
            assert!(is_safe_git_name(ok), "{ok}");
        }
        for bad in ["-x", "a..b", "a b", "a~", "a^", "a:b", "a?", "a*", "a[", "x/", "x.lock", "a\\b"] {
            assert!(!is_safe_git_name(bad), "{bad}");
        }
    }
}
