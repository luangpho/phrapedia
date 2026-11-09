//! One chat turn (port of server/utils/agent/index.ts): store the user message, run `claude -p`, store the assistant message.
//! The tool-calling loop happens inside the CLI, which calls back into the app's MCP server (`phrapedia mcp`, stdio);
//! Gemini is only reachable through the web_research tool.

use super::claude::*;
use super::history::with_history_digest;
use super::prompt::{build_system_prompt, SystemPromptInput};
use super::stream::ChatStreamEvent;
use crate::core::entities::{assert_kind, assert_slug};
use crate::core::error::{AppError, Result};
use crate::core::types::{ChatMessage, ChatPart, ChatRole, ChatSession, EntityKind};
use crate::core::Core;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

pub const NO_CLAUDE_CLI_MESSAGE: &str = "Claude Code CLI（claude）が見つかりません。Claude Code をインストールし、ターミナルで `claude` を実行してログインしてください。見つからない場合は 設定 →「チャット AI」でコマンドのパス（例: /opt/homebrew/bin/claude）を指定できます。";
pub const CLAUDE_LOGIN_MESSAGE: &str = "Claude Code CLI にログインしていないようです。ターミナルで `claude` を実行してログインしてから、もう一度お試しください。";

/// `shared/types/chat.ts` ChatRequest + `turnId`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub turn_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    pub kind: EntityKind,
    #[serde(default)]
    pub slug: Option<String>,
    pub message: String,
    #[serde(default)]
    pub draft: String,
}

/// Input validation of the old POST /api/chat (these reject; everything later is reported through events).
pub fn validate_request(req: &ChatRequest) -> Result<(String, Option<String>)> {
    if req.turn_id.trim().is_empty() {
        return Err(AppError::invalid("turnId が必要です"));
    }
    assert_kind(req.kind.as_str())?;
    let slug = match req.slug.as_deref() {
        Some(s) if !s.is_empty() => Some(assert_slug(s)?),
        _ => None,
    };
    let message = req.message.trim().to_string();
    if message.is_empty() {
        return Err(AppError::invalid("message が必要です"));
    }
    Ok((message, slug))
}

pub struct Prepared {
    pub session: ChatSession,
    pub user_message: ChatMessage,
    pub assistant_id: String,
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Prepare the session (create / load) and the message ids before any provider work starts.
pub fn prepare_chat_turn(core: &Core, req: &ChatRequest, message: &str, slug: Option<&str>) -> Result<Prepared> {
    let settings = core.settings();
    let probe = core.probe_claude(&settings.ai.command);
    if !probe.found {
        return Err(AppError::new(crate::core::error::ErrorCode::CliNotFound, NO_CLAUDE_CLI_MESSAGE));
    }
    let mut session = match req.session_id.as_deref().filter(|s| !s.is_empty()) {
        Some(id) => core.get_chat_session(id)?,
        None => None,
    };
    if session.as_ref().is_some_and(|s| s.kind != req.kind) {
        session = None;
    }
    let session = match session {
        Some(mut s) => {
            if let Some(slug) = slug {
                if s.slug.as_deref() != Some(slug) {
                    // the draft was saved in the meantime: bind the conversation to the record
                    core.touch_chat_session(&s.id, Some(Some(slug)), None)?;
                    s.slug = Some(slug.to_string());
                }
            }
            s
        }
        None => {
            let title: String = message.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(60).collect();
            core.create_chat_session(req.kind, slug, if title.is_empty() { "新しい会話" } else { &title })?
        }
    };
    let user_message = ChatMessage { id: new_id(), role: ChatRole::User, parts: vec![ChatPart::Text { text: message.to_string() }], created_at: now_iso() };
    Ok(Prepared { session, user_message, assistant_id: new_id() })
}

/// Japanese, user-facing message for a failed CLI run.
pub fn describe_claude_failure(run: &ClaudeProcessResult, command: &str, timeout_ms: u64, model: &str) -> String {
    if let Some(e) = &run.spawn_error {
        return format!("{NO_CLAUDE_CLI_MESSAGE}（{command}: {e}）");
    }
    if run.timed_out {
        return format!("Claude Code CLI が {} 秒以内に終わらなかったため中断しました。設定のタイムアウトを延ばすか、依頼を分けてお試しください。", (timeout_ms as f64 / 1000.0).round() as u64);
    }
    let detail = run.run.result_error.clone().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| run.stderr.clone());
    let detail = detail.trim();
    let lower = detail.to_lowercase();
    let any = |keys: &[&str]| keys.iter().any(|k| lower.contains(k));
    let clip = |n: usize| detail.chars().take(n).collect::<String>();
    if any(&["not logged in", "please run /login", "/login", "invalid api key", "authentication", "unauthorized", "unauthorised", "oauth", "credential"]) {
        return CLAUDE_LOGIN_MESSAGE.to_string();
    }
    if lower.contains("model") && any(&["not found", "unknown", "invalid", "unrecognized", "does not exist", "issue with the selected model"]) {
        return format!("モデル「{model}」を Claude Code CLI が受け付けませんでした。設定のモデル名を確認してください。{}", if detail.is_empty() { String::new() } else { format!("（{}）", clip(300)) });
    }
    if any(&["rate limit", "usage limit", "overloaded", "429", "529"]) {
        return format!("Claude の利用上限または混雑のため応答できませんでした。しばらく待ってから再試行してください。（{}）", clip(300));
    }
    if !detail.is_empty() {
        return format!("Claude Code CLI のエラー: {}", clip(600));
    }
    format!("Claude Code CLI が異常終了しました（exit code {}）。", run.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "?".into()))
}

/// `--resume` of a session the CLI no longer has (other machine, session file gone) → start over with a digest.
pub fn is_resume_failure(run: &ClaudeProcessResult) -> bool {
    if !run.run.parts.is_empty() {
        return false;
    }
    let text = format!("{}\n{}", run.run.result_error.as_deref().unwrap_or(""), run.stderr).to_lowercase();
    text.contains("no conversation found") || (text.contains("session") && (text.contains("not found") || text.contains("does not exist")))
}

pub struct TurnOutcome {
    pub assistant_message: ChatMessage,
    pub error: Option<String>,
    pub aborted: bool,
}

struct Attempt<'a> {
    settings: &'a crate::core::app_settings::AppSettings,
    files: &'a RunFiles,
    tool_names: &'a [String],
    work_dir: &'a std::path::Path,
    env: &'a [(String, String)],
    env_remove: &'a [&'static str],
    abort: &'a watch::Receiver<bool>,
}

impl Attempt<'_> {
    async fn run(&self, session_id: Option<&str>, resume: Option<&str>, prompt: String, emit: &mut (dyn FnMut(ChatStreamEvent) + Send)) -> ClaudeProcessResult {
        let args = build_claude_args(&ClaudeArgsInput {
            model: &self.settings.ai.model,
            system_prompt_file: &self.files.system_prompt,
            mcp_config_file: &self.files.mcp_config,
            tool_names: self.tool_names,
            session_id,
            resume,
            extra_args: &self.settings.ai.args,
        });
        let opts = ClaudeProcessOptions {
            command: &self.settings.ai.command,
            args,
            prompt,
            cwd: self.work_dir.to_path_buf(),
            env: self.env.to_vec(),
            env_remove: self.env_remove.iter().map(|s| s.to_string()).collect(),
            timeout: Duration::from_millis(self.settings.ai.timeout_ms),
            abort: self.abort.clone(),
            kill_grace: KILL_GRACE,
        };
        run_claude_process(opts, emit).await
    }
}

/// Run one chat turn. `emit` receives every `ChatStreamEvent` except the final `done` (the caller sends it from the outcome).
pub async fn run_chat_turn(
    core: &Arc<Core>,
    req: &ChatRequest,
    prepared: Prepared,
    mcp: &McpLaunch,
    abort: watch::Receiver<bool>,
    emit: &mut (dyn FnMut(ChatStreamEvent) + Send),
) -> Result<TurnOutcome> {
    let settings = core.settings();
    let Prepared { session, user_message, assistant_id } = prepared;
    let history = core.list_chat_messages(&session.id)?;
    core.append_chat_message(&session.id, &user_message)?;

    let tool_names: Vec<String> = crate::mcp::tools::TOOL_NAMES.iter().filter(|n| !CHAT_DENIED_TOOLS.contains(n)).map(|s| s.to_string()).collect();
    let system = build_system_prompt(&SystemPromptInput { kind: req.kind, slug: session.slug.as_deref(), draft: &req.draft, settings: &settings, tool_names: &tool_names });
    let work_dir = claude_work_dir(&core.dirs.data_dir)?;
    sweep_stale_runs(&work_dir);
    // System prompt (holds the live draft) and MCP config go through 0600 files, never argv (`ps` shows argv).
    let files = RunFiles::create(&work_dir, &system, &build_mcp_config(mcp))?;
    let (env, env_remove) = claude_env(settings.ai.max_tokens);

    let attempt = Attempt { settings: &settings, files: &files, tool_names: &tool_names, work_dir: &work_dir, env: &env, env_remove: &env_remove, abort: &abort };

    // First turn: --session-id <new uuid>. Later turns: --resume <id>. When resume fails start over with a new session
    // and hand the stored history over as a digest.
    let run = match session.claude_session_id.as_deref().filter(|s| !s.is_empty()) {
        Some(resume) => {
            let first = attempt.run(None, Some(resume), req.message.trim().to_string(), emit).await;
            if is_resume_failure(&first) && !first.aborted {
                let id = new_id();
                attempt.run(Some(&id), None, with_history_digest(req.message.trim(), &history), emit).await
            } else {
                first
            }
        }
        None => {
            let id = new_id();
            let prompt = if history.is_empty() { req.message.trim().to_string() } else { with_history_digest(req.message.trim(), &history) };
            attempt.run(Some(&id), None, prompt, emit).await
        }
    };
    drop(files);

    let failed = !run.aborted && (run.spawn_error.is_some() || run.timed_out || run.run.result_error.is_some() || (!run.run.finished && run.exit_code != Some(0)));
    let mut error = None;
    if failed {
        let msg = describe_claude_failure(&run, &settings.ai.command, settings.ai.timeout_ms, &settings.ai.model);
        emit(ChatStreamEvent::Error { message: msg.clone() });
        error = Some(msg);
    }
    // Remember the CLI session only when the CLI actually created it.
    if let Some(id) = &run.run.session_id {
        core.set_claude_session_id(&session.id, Some(id))?;
    }
    let mut parts = run.run.parts.clone();
    if run.aborted {
        parts.push(ChatPart::Text { text: "（ユーザーが停止しました）".into() });
    } else if let (Some(e), true) = (&error, parts.is_empty()) {
        parts.push(ChatPart::Text { text: format!("エラー: {e}") });
    }
    let assistant_message = ChatMessage { id: assistant_id, role: ChatRole::Assistant, parts, created_at: now_iso() };
    core.append_chat_message(&session.id, &assistant_message)?;
    core.touch_chat_session(&session.id, None, None)?;
    Ok(TurnOutcome { assistant_message, error, aborted: run.aborted })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::stream::ClaudeRunResult;

    fn req(msg: &str) -> ChatRequest {
        ChatRequest { turn_id: "t".into(), session_id: None, kind: EntityKind::Monks, slug: None, message: msg.into(), draft: String::new() }
    }
    fn failed(result_error: Option<&str>, stderr: &str) -> ClaudeProcessResult {
        ClaudeProcessResult { run: ClaudeRunResult { result_error: result_error.map(str::to_string), ..Default::default() }, stderr: stderr.into(), ..Default::default() }
    }

    #[test]
    fn request_deserializes_the_frontend_shape() {
        let r: ChatRequest = serde_json::from_value(serde_json::json!({"turnId":"u","kind":"temples","slug":null,"message":"hi","draft":"d"})).unwrap();
        assert_eq!((r.turn_id.as_str(), r.kind, r.slug, r.session_id, r.draft.as_str()), ("u", EntityKind::Temples, None, None, "d"));
    }

    #[test]
    fn validation_rejects_empty_message_turn_id_and_bad_slug() {
        assert!(validate_request(&req("  ")).is_err());
        let mut r = req("x");
        r.turn_id = " ".into();
        assert!(validate_request(&r).is_err());
        let mut r = req("x");
        r.slug = Some("../evil".into());
        assert_eq!(validate_request(&r).unwrap_err().code, crate::core::error::ErrorCode::InvalidArgument);
        let mut r = req(" hello ");
        r.slug = Some(String::new());
        assert_eq!(validate_request(&r).unwrap(), ("hello".to_string(), None));
    }

    #[test]
    fn resume_failure_needs_no_parts_and_a_missing_session_message() {
        assert!(is_resume_failure(&failed(Some("No conversation found with session ID: abc"), "")));
        assert!(is_resume_failure(&failed(None, "Error: session abc not found")));
        assert!(!is_resume_failure(&failed(Some("rate limit"), "")));
        let mut with_parts = failed(Some("No conversation found"), "");
        with_parts.run.parts.push(ChatPart::Text { text: "x".into() });
        assert!(!is_resume_failure(&with_parts));
    }

    #[test]
    fn failure_messages_are_classified() {
        let d = |r: &ClaudeProcessResult| describe_claude_failure(r, "claude", 600_000, "claude-x");
        assert!(d(&ClaudeProcessResult { spawn_error: Some("ENOENT".into()), ..Default::default() }).contains("見つかりません"));
        assert!(d(&ClaudeProcessResult { timed_out: true, ..Default::default() }).contains("600 秒以内"));
        assert_eq!(d(&failed(Some("Not logged in · Please run /login"), "")), CLAUDE_LOGIN_MESSAGE);
        assert!(d(&failed(Some("There's an issue with the selected model (x). It may not exist"), "")).contains("モデル「claude-x」"));
        assert!(d(&failed(None, "429 rate limit")).contains("利用上限"));
        assert!(d(&failed(None, "boom")).starts_with("Claude Code CLI のエラー: boom"));
        assert!(d(&ClaudeProcessResult { exit_code: Some(2), ..Default::default() }).contains("exit code 2"));
    }
}
