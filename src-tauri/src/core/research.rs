//! Web research through the Antigravity (`agy`) / Gemini CLI (port of server/utils/gemini.ts).
//! Timeout, result cache (SQLite), JSON-envelope unwrapping and primary-source image staging.

use super::app_settings::ResearchEngine;
use super::cli::{run_capture, RunOpts};
use super::error::{AppError, ErrorCode, Result};
use super::paths::safe_join;
use super::types::ResearchResult;
use super::Core;
use rusqlite::{params, OptionalExtension};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub fn engine_label(e: ResearchEngine) -> &'static str {
    match e {
        ResearchEngine::Antigravity => "Antigravity CLI (agy)",
        ResearchEngine::GeminiCli => "Gemini CLI (gemini)",
    }
}

pub fn default_command(e: ResearchEngine) -> &'static str {
    match e {
        ResearchEngine::Antigravity => "agy",
        ResearchEngine::GeminiCli => "gemini",
    }
}

#[derive(Debug, Default, Clone)]
pub struct ResearchRun {
    pub prompt: String,
    /// Primary-source image paths (relative to the sources dir)
    pub images: Vec<String>,
    pub model: Option<String>,
    pub timeout_ms: Option<u64>,
    pub no_cache: bool,
}

#[derive(Debug, Default, Clone)]
pub struct PromptOptions {
    pub context: Option<String>,
    pub images: Vec<String>,
    pub ocr_text: Option<String>,
}

/// argv (and optional stdin) for a headless run.
/// - agy: the prompt goes in `-p`; `--output-format json`, optional `--model`, approval mode fixed to `--mode auto`.
/// - gemini: the prompt is sent on stdin, `-p` carries a short trailer; `--output-format json`, `-m`, `--approval-mode auto_edit`, `--skip-trust`.
pub fn build_cli_invocation(engine: ResearchEngine, model: &str, extra: &[String], prompt: &str) -> (Vec<String>, Option<String>) {
    let has = |f: &str| extra.iter().any(|a| a == f);
    let mut args: Vec<String> = vec![];
    match engine {
        ResearchEngine::Antigravity => {
            if !model.is_empty() && !has("--model") {
                args.extend(["--model".into(), model.into()]);
            }
            args.extend(extra.iter().cloned());
            if !has("--output-format") {
                args.extend(["--output-format".into(), "json".into()]);
            }
            if !has("--mode") && !has("--dangerously-skip-permissions") {
                args.extend(["--mode".into(), "auto".into()]);
            }
            args.extend(["-p".into(), prompt.into()]);
            (args, None)
        }
        ResearchEngine::GeminiCli => {
            if !model.is_empty() && !has("-m") && !has("--model") {
                args.extend(["-m".into(), model.into()]);
            }
            args.extend(extra.iter().cloned());
            if !has("--output-format") {
                args.extend(["--output-format".into(), "json".into()]);
            }
            if !has("--approval-mode") && !has("--yolo") && !has("-y") {
                args.extend(["--approval-mode".into(), "auto_edit".into()]);
            }
            if !has("--skip-trust") {
                args.push("--skip-trust".into());
            }
            args.extend(["-p".into(), "上記の指示に従って回答してください。".into()]);
            (args, Some(prompt.to_string()))
        }
    }
}

/// Accepts the JSON envelope of either CLI (possibly preceded by log lines) or raw text.
/// - Gemini CLI: `{ session_id, response, stats, error? }`; Antigravity CLI: `{ conversation_id, status, response, error?, finished_reason? }`
pub fn unwrap_output(stdout: &str) -> Result<String> {
    let trimmed = stdout.trim();
    if let Some(start) = trimmed.find('{') {
        if let Ok(Value::Object(m)) = serde_json::from_str::<Value>(&trimmed[start..]) {
            let answer = ["response", "result", "text"].iter().find_map(|k| m.get(*k).and_then(Value::as_str));
            if let Some(err) = m.get("error").filter(|e| !e.is_null()) {
                if answer.map(str::trim).unwrap_or("").is_empty() {
                    let msg = err.as_str().map(str::to_string).or_else(|| err.get("message").and_then(Value::as_str).map(str::to_string));
                    return Err(AppError::new(ErrorCode::CliFailed, "CLI がエラーを返しました").with_detail(msg.unwrap_or_else(|| "unknown error".into())));
                }
            }
            if let Some(a) = answer {
                return Ok(match m.get("finished_reason").and_then(Value::as_str) {
                    Some(r) if r != "stop" => format!("{a}\n\n（注: CLI の応答は途中で終了しました: {r}）"),
                    _ => a.to_string(),
                });
            }
        }
    }
    Ok(trimmed.to_string())
}

fn exit_hint(engine: ResearchEngine, code: Option<i32>) -> &'static str {
    match (engine, code) {
        (ResearchEngine::GeminiCli, Some(41)) => " (認証エラー: GEMINI_API_KEY を設定するか、このマシンで一度 `gemini` を実行してログインしてください)",
        (ResearchEngine::GeminiCli, Some(42)) => " (入力エラー)",
        (ResearchEngine::Antigravity, Some(3)) => " (モデル/エージェントのエラー、または --print-timeout による打ち切り)",
        (ResearchEngine::Antigravity, Some(1)) => " (起動エラー: 未ログインの可能性。`agy` を一度対話起動して Google にサインインするか、GEMINI_API_KEY と settings.json の modelProvider: \"gemini\" を設定してください)",
        _ => "",
    }
}

/// The research prompt used by the web_research tool.
pub fn build_research_prompt(query: &str, o: &PromptOptions) -> String {
    let mut parts: Vec<String> = vec![
        "あなたはタイ仏教（高僧・寺院・プラクルアン）の調査アシスタントです。".into(),
        "必ず Google 検索ツールでウェブを検索し、以下の質問に日本語で答えてください。".into(),
        "出力は Markdown。事実ごとに出典URLを [タイトル](URL) 形式で明記し、確認できない事項は「未確認」と書いてください。".into(),
        "タイ語の固有名詞は原語（タイ語）と、分かればローマ字表記も併記してください。".into(),
    ];
    if !o.images.is_empty() {
        let names: Vec<String> = (1..=o.images.len()).map(|i| format!("image-{i}")).collect();
        parts.push(String::new());
        parts.push("## 画像の取り扱い（必須）".into());
        parts.push(format!("この質問には画像 {} 件が添付されています（作業ディレクトリ直下の {}。元ファイル: {}）。", o.images.len(), names.join(", "), o.images.join(", ")));
        parts.extend([
            "1. 添付画像本体を必ず自分で開いて確認してください（ファイル読み取りツールで画像を読む）。画像を見ずに回答してはいけません。",
            "2. 画像内の文字（タイ語・日本語・英語・数字・梵字など）を必ず自分で読み取ってください。下の「参考: 事前に読み取られた文字」はあくまで参考情報であり、確定ではありません。自分の読み取り結果と照合し、相違があれば両方を示してください。",
            "3. 読み取った文字と、参考テキストや検索結果の名称・寺院名・年号などが完全に一致すると確証できない場合は、画像を拡大する、文字の部分ごとに切り出して読み直す、向きを変えて確認するなどして必ず読み直しを行ってください。それでも不確かな部分は候補を複数挙げ、確信度（高・中・低）を付けてください。",
            "4. 読み取れた文字列（原語）をそのまま検索語にして Google 検索し、画像の対象（高僧・寺院・プラクルアンの名称、寺院、製作年、種類など）を特定してください。",
            "5. 回答の冒頭に「## 画像から読み取った文字」の節を置き、読み取り結果・確信度・参考テキストとの差異を記載してから、調査結果を書いてください。",
        ].map(String::from));
        if let Some(ocr) = o.ocr_text.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            parts.extend([String::new(), "### 参考: 事前に読み取られた文字（参考情報・要照合）".into(), ocr.to_string()]);
        }
    }
    if let Some(c) = o.context.as_deref().filter(|c| !c.is_empty()) {
        parts.extend([String::new(), "## 既知の情報（一次資料より・これを優先）".into(), c.to_string()]);
    }
    parts.extend([String::new(), "## 質問".into(), query.to_string()]);
    parts.join("\n")
}

struct Staged {
    dir: PathBuf,
    name: String,
    hash: String,
}

const IMAGE_EXTS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];

impl Core {
    /// Copy primary-source images into a fresh per-run work directory (the CLI runs there, so it can open them).
    fn stage_images(&self, paths: &[String]) -> Result<Vec<Staged>> {
        if paths.is_empty() {
            return Ok(vec![]);
        }
        let base = self.sources_dir()?;
        let mut files = vec![];
        for rel in paths.iter().take(8) {
            let full = safe_join(&base, rel)?;
            let ext = full.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if !IMAGE_EXTS.contains(&ext.as_str()) {
                return Err(AppError::invalid(format!("画像ファイルではありません: {rel}")));
            }
            let data = fs::read(&full).map_err(|_| AppError::not_found(format!("画像が見つかりません: {rel}")))?;
            files.push((ext, full, data));
        }
        let dir = self.dirs.state_dir().join("gemini-work").join(format!("run-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&dir).map_err(|e| AppError::io("作業フォルダを作れません", e))?;
        let mut out = vec![];
        for (i, (ext, src, data)) in files.into_iter().enumerate() {
            let name = format!("image-{}.{ext}", i + 1);
            if let Err(e) = fs::copy(&src, dir.join(&name)) {
                let _ = fs::remove_dir_all(&dir);
                return Err(AppError::io("画像をコピーできません", e));
            }
            let hash = format!("{:x}", Sha256::digest(&data));
            out.push(Staged { dir: dir.clone(), name, hash: hash[..16].to_string() });
        }
        Ok(out)
    }

    /// Run the configured Google CLI headlessly and return the answer (cached per model + images + prompt).
    pub fn run_research(&self, opts: &ResearchRun) -> Result<ResearchResult> {
        let s = self.settings();
        let engine = s.gemini.engine;
        let command = if s.gemini.command.trim().is_empty() { default_command(engine).to_string() } else { s.gemini.command.clone() };
        let model = opts.model.clone().unwrap_or_else(|| s.gemini.model.clone());
        let timeout = Duration::from_millis(opts.timeout_ms.unwrap_or(s.gemini.timeout_ms));
        let cache_hours = s.gemini.cache_hours;
        let use_cache = !opts.no_cache && cache_hours > 0.0;
        let staged = self.stage_images(&opts.images)?;
        let key = {
            let hashes: Vec<&str> = staged.iter().map(|a| a.hash.as_str()).collect();
            format!("{:x}", Sha256::digest(format!("{model}\n{}\n{}", hashes.join(","), opts.prompt).as_bytes()))
        };
        let label = engine_label(engine).to_string();
        let cleanup = |staged: &[Staged]| {
            if let Some(a) = staged.first() {
                let _ = fs::remove_dir_all(&a.dir);
            }
        };

        if use_cache {
            let hit = self.with_db(|c| {
                Ok(c.query_row("SELECT result, model, created_at FROM research_cache WHERE query_hash = ?1", [&key], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?))).optional()?)
            })?;
            if let Some((result, m, created)) = hit {
                let fresh = chrono::DateTime::parse_from_rfc3339(&created).map(|t| (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_milliseconds() as f64 / 3_600_000.0 < cache_hours).unwrap_or(false);
                if fresh {
                    cleanup(&staged);
                    return Ok(ResearchResult { text: result, engine: Some(label), model: m, cached: true, duration_ms: 0, stderr: None });
                }
            }
        }

        // Gemini CLI embeds `@file` references straight into the prompt; Antigravity reads them with its file tool.
        let prompt = if staged.is_empty() {
            opts.prompt.clone()
        } else {
            format!("{}\n\n添付画像ファイル（作業ディレクトリ直下）: {}", opts.prompt, staged.iter().map(|a| format!("@{}", a.name)).collect::<Vec<_>>().join(" "))
        };
        let (args, stdin) = build_cli_invocation(engine, &model, &s.gemini.args, &prompt);
        // Empty work dir so the project's own GEMINI.md / AGENTS.md / .gemini settings are not picked up.
        let cwd = staged.first().map(|a| a.dir.clone()).unwrap_or_else(|| self.dirs.state_dir().join("gemini-work"));
        let _ = fs::create_dir_all(&cwd);
        let started = Instant::now();
        let run = run_capture(
            &command,
            &args,
            &RunOpts {
                cwd: Some(cwd),
                env: [("NO_COLOR", "1"), ("CI", "true"), ("GEMINI_CLI_NO_RELAUNCH", "1"), ("GEMINI_CLI_TRUST_WORKSPACE", "true"), ("GEMINI_CLI_SURFACE", "phrapedia")].map(|(k, v)| (k.to_string(), v.to_string())).to_vec(),
                stdin,
                timeout,
                ..Default::default()
            },
        );
        cleanup(&staged);
        let out = run.map_err(|e| match e.code {
            ErrorCode::CliNotFound => e.with_detail("Antigravity CLI: curl -fsSL https://antigravity.google/cli/install.sh | bash / Gemini CLI: npm i -g @google/gemini-cli"),
            _ => e,
        })?;
        let duration_ms = started.elapsed().as_millis() as u64;
        let stdout = out.stdout_text();
        let stderr = out.stderr_text();
        if out.code != Some(0) && stdout.trim().is_empty() {
            let tail: String = stderr.trim().chars().rev().take(2000).collect::<Vec<_>>().into_iter().rev().collect();
            return Err(AppError::new(ErrorCode::CliFailed, format!("{label} が失敗しました (終了コード {}){}", out.code.map(|c| c.to_string()).unwrap_or_else(|| "なし".into()), exit_hint(engine, out.code))).with_detail(tail));
        }
        let text = unwrap_output(&stdout).map_err(|e| AppError { message: format!("{}{}", e.message, exit_hint(engine, out.code)), ..e })?;
        if text.trim().is_empty() {
            return Err(AppError::new(ErrorCode::CliFailed, format!("{label} の出力が空でした{}", exit_hint(engine, out.code))).with_detail(stderr.trim().chars().take(2000).collect::<String>()));
        }
        if use_cache {
            self.with_db(|c| {
                c.execute(
                    "INSERT INTO research_cache (query_hash, query, result, model, created_at) VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT (query_hash) DO UPDATE SET result = excluded.result, model = excluded.model, created_at = excluded.created_at",
                    params![key, opts.prompt, text, if model.is_empty() { None } else { Some(model.clone()) }, super::entities::now_iso()],
                )?;
                Ok(())
            })?;
        }
        let stderr_trim: String = stderr.trim().chars().take(1000).collect();
        Ok(ResearchResult { text, engine: Some(label), model: if model.is_empty() { None } else { Some(model) }, cached: false, duration_ms, stderr: if stderr_trim.is_empty() { None } else { Some(stderr_trim) } })
    }

    /// REST `POST /api/research` / MCP `web_research`: builds the prompt, then runs it.
    pub fn web_research(&self, query: &str, context: Option<String>, images: Vec<String>, ocr_text: Option<String>, no_cache: bool) -> Result<ResearchResult> {
        if query.trim().is_empty() {
            return Err(AppError::invalid("query が必要です"));
        }
        let images: Vec<String> = images.into_iter().take(8).collect();
        let prompt = build_research_prompt(query, &PromptOptions { context, images: images.clone(), ocr_text });
        self.run_research(&ResearchRun { prompt, images, no_cache, ..Default::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::core_with_dataset;
    use super::*;
    use serde_json::json;

    #[test]
    fn agy_invocation() {
        let (args, stdin) = build_cli_invocation(ResearchEngine::Antigravity, "flash", &[], "PROMPT");
        assert_eq!(args, ["--model", "flash", "--output-format", "json", "--mode", "auto", "-p", "PROMPT"]);
        assert!(stdin.is_none());
        let (args, _) = build_cli_invocation(ResearchEngine::Antigravity, "flash", &["--model".into(), "pro".into(), "--mode".into(), "plan".into(), "--output-format".into(), "text".into()], "P");
        assert_eq!(args, ["--model", "pro", "--mode", "plan", "--output-format", "text", "-p", "P"]);
        let (args, _) = build_cli_invocation(ResearchEngine::Antigravity, "", &["--dangerously-skip-permissions".into()], "P");
        assert!(!args.contains(&"--mode".to_string()));
    }

    #[test]
    fn gemini_invocation_uses_stdin() {
        let (args, stdin) = build_cli_invocation(ResearchEngine::GeminiCli, "pro", &[], "PROMPT");
        assert_eq!(args, ["-m", "pro", "--output-format", "json", "--approval-mode", "auto_edit", "--skip-trust", "-p", "上記の指示に従って回答してください。"]);
        assert_eq!(stdin.as_deref(), Some("PROMPT"));
        let (args, _) = build_cli_invocation(ResearchEngine::GeminiCli, "pro", &["-m".into(), "x".into(), "--yolo".into(), "--skip-trust".into()], "P");
        assert_eq!(args.iter().filter(|a| *a == "-m").count(), 1);
        assert!(!args.contains(&"--approval-mode".to_string()));
    }

    #[test]
    fn unwraps_envelopes() {
        assert_eq!(unwrap_output(r#"{"response":"答え","conversation_id":"c"}"#).unwrap(), "答え");
        assert_eq!(unwrap_output("log line\n{\"response\":\"ok\"}\n").unwrap(), "ok");
        assert_eq!(unwrap_output(r#"{"result":"r"}"#).unwrap(), "r");
        assert_eq!(unwrap_output("plain text answer\n").unwrap(), "plain text answer");
        assert!(unwrap_output(r#"{"response":"x","finished_reason":"max_tokens"}"#).unwrap().contains("max_tokens"));
        assert_eq!(unwrap_output(r#"{"response":"x","finished_reason":"stop"}"#).unwrap(), "x");
        let e = unwrap_output(r#"{"error":{"message":"quota"}}"#).unwrap_err();
        assert_eq!(e.code, ErrorCode::CliFailed);
        assert_eq!(e.detail.as_deref(), Some("quota"));
        assert!(unwrap_output(r#"{"error":"boom","response":"  "}"#).is_err());
        assert_eq!(unwrap_output(r#"{"error":"warn","response":"still answered"}"#).unwrap(), "still answered");
        // not valid JSON after the first brace: treated as text
        assert_eq!(unwrap_output("see {braces} here").unwrap(), "see {braces} here");
    }

    #[test]
    fn prompt_sections() {
        let p = build_research_prompt("ルアンポー・トーの生年", &PromptOptions::default());
        assert!(p.starts_with("あなたはタイ仏教"));
        assert!(p.ends_with("## 質問\nルアンポー・トーの生年"));
        assert!(!p.contains("画像の取り扱い") && !p.contains("既知の情報"));
        let p = build_research_prompt("q", &PromptOptions { context: Some("既知".into()), images: vec!["a/x.png".into(), "y.jpg".into()], ocr_text: Some("  วัด  ".into()) });
        assert!(p.contains("画像 2 件が添付されています（作業ディレクトリ直下の image-1, image-2。元ファイル: a/x.png, y.jpg）"));
        assert!(p.contains("### 参考: 事前に読み取られた文字（参考情報・要照合）\nวัด\n"));
        assert!(p.contains("## 既知の情報（一次資料より・これを優先）\n既知"));
        assert!(p.find("画像の取り扱い").unwrap() < p.find("既知の情報").unwrap());
    }

    #[cfg(unix)]
    fn fake_cli(dir: &std::path::Path, script: &str) -> String {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("fake-agy");
        fs::write(&p, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        p.to_string_lossy().into_owned()
    }

    #[cfg(unix)]
    #[test]
    fn runs_caches_and_reports_failures() {
        let (t, core) = core_with_dataset();
        let counter = t.path().join("count");
        let cli = fake_cli(t.path(), &format!("echo x >> '{}'\nprintf '%s' '{{\"response\":\"調査結果\"}}'", counter.display()));
        core.update_settings(&json!({ "gemini": { "command": cli, "model": "flash", "cacheHours": 1 } })).unwrap();
        let r = core.web_research("q1", None, vec![], None, false).unwrap();
        assert_eq!((r.text.as_str(), r.cached, r.model.as_deref(), r.engine.as_deref()), ("調査結果", false, Some("flash"), Some("Antigravity CLI (agy)")));
        let r2 = core.web_research("q1", None, vec![], None, false).unwrap();
        assert!(r2.cached && r2.text == "調査結果" && r2.duration_ms == 0);
        assert_eq!(fs::read_to_string(&counter).unwrap().lines().count(), 1, "second call served from cache");
        let r3 = core.web_research("q1", None, vec![], None, true).unwrap();
        assert!(!r3.cached);
        assert_eq!(fs::read_to_string(&counter).unwrap().lines().count(), 2, "noCache bypasses");
        core.web_research("q2", None, vec![], None, false).unwrap();
        assert_eq!(fs::read_to_string(&counter).unwrap().lines().count(), 3, "different prompt = different key");

        // cacheHours 0 disables the cache entirely
        core.update_settings(&json!({ "gemini": { "cacheHours": 0 } })).unwrap();
        core.web_research("q1", None, vec![], None, false).unwrap();
        assert_eq!(fs::read_to_string(&counter).unwrap().lines().count(), 4);
    }

    #[cfg(unix)]
    #[test]
    fn failure_modes() {
        let (t, core) = core_with_dataset();
        let cli = fake_cli(t.path(), "echo 'login required' >&2; exit 1");
        core.update_settings(&json!({ "gemini": { "command": cli } })).unwrap();
        let e = core.web_research("q", None, vec![], None, true).unwrap_err();
        assert_eq!(e.code, ErrorCode::CliFailed);
        assert!(e.message.contains("終了コード 1") && e.message.contains("未ログイン"), "{}", e.message);
        assert!(e.detail.unwrap().contains("login required"));

        let cli = fake_cli(t.path(), "exit 0");
        core.update_settings(&json!({ "gemini": { "command": cli } })).unwrap();
        assert_eq!(core.web_research("q", None, vec![], None, true).unwrap_err().code, ErrorCode::CliFailed, "empty output");

        let cli = fake_cli(t.path(), "sleep 20");
        core.update_settings(&json!({ "gemini": { "command": cli, "timeoutMs": 1000 } })).unwrap();
        let t0 = Instant::now();
        assert_eq!(core.web_research("q", None, vec![], None, true).unwrap_err().code, ErrorCode::Timeout);
        assert!(t0.elapsed() < Duration::from_secs(15));

        core.update_settings(&json!({ "gemini": { "command": "/nonexistent/agy" } })).unwrap();
        assert_eq!(core.web_research("q", None, vec![], None, true).unwrap_err().code, ErrorCode::CliNotFound);
        assert_eq!(core.web_research("  ", None, vec![], None, true).unwrap_err().code, ErrorCode::InvalidArgument);
    }

    #[cfg(unix)]
    #[test]
    fn gemini_engine_reads_prompt_from_stdin() {
        let (t, core) = core_with_dataset();
        let cli = fake_cli(t.path(), "IN=$(cat)\nprintf '{\"response\":\"stdin:%s\"}' \"$(printf '%s' \"$IN\" | tail -1)\"");
        core.update_settings(&json!({ "gemini": { "engine": "gemini-cli", "command": cli } })).unwrap();
        let r = core.web_research("最後の行", None, vec![], None, true).unwrap();
        assert_eq!(r.text, "stdin:最後の行");
        assert_eq!(r.engine.as_deref(), Some("Gemini CLI (gemini)"));
    }

    #[cfg(unix)]
    #[test]
    fn images_are_staged_in_a_work_dir_and_cleaned_up() {
        let (t, core) = core_with_dataset();
        let src = core.dataset_root().unwrap().join("sources/img");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.png"), b"PNGDATA").unwrap();
        fs::write(src.join("note.md"), b"x").unwrap();
        let cli = fake_cli(t.path(), "pwd > \"$HOME_PROBE\"; ls >> \"$HOME_PROBE\"; printf '%s' '{\"response\":\"ok\"}'");
        let probe = t.path().join("probe");
        // the fake CLI learns where to write through the settings args (no env passthrough needed)
        let cli = cli.replace("$HOME_PROBE", &probe.to_string_lossy());
        fs::write(&cli, fs::read_to_string(&cli).unwrap().replace("$HOME_PROBE", &probe.to_string_lossy())).unwrap();
        core.update_settings(&json!({ "gemini": { "command": cli } })).unwrap();
        let r = core.web_research("q", None, vec!["img/a.png".into()], Some("ocr".into()), true).unwrap();
        assert_eq!(r.text, "ok");
        let seen = fs::read_to_string(&probe).unwrap();
        assert!(seen.contains("gemini-work/run-") && seen.contains("image-1.png"), "{seen}");
        let work = core.dirs.state_dir().join("gemini-work");
        assert_eq!(fs::read_dir(&work).unwrap().count(), 0, "run dir is removed after the run");

        assert_eq!(core.web_research("q", None, vec!["img/note.md".into()], None, true).unwrap_err().code, ErrorCode::InvalidArgument);
        assert_eq!(core.web_research("q", None, vec!["img/none.png".into()], None, true).unwrap_err().code, ErrorCode::NotFound);
        assert_eq!(core.web_research("q", None, vec!["../x.png".into()], None, true).unwrap_err().code, ErrorCode::InvalidArgument);
        assert_eq!(fs::read_dir(&work).unwrap().count(), 0, "a rejected image leaves no work dir behind");
    }
}
