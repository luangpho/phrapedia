//! `claude -p` runner: argv / env / private run dir (system prompt + MCP config as 0600 files, never in argv),
//! process-group spawn, stream parsing, abort / timeout with SIGTERM → SIGKILL on the whole group.
//!
//! Flags (all verified on Claude Code 2.1.317, see docs/reports/rust-native.md):
//!  -p, --output-format stream-json, --verbose, --include-partial-messages, --model,
//!  --system-prompt-file (the system prompt carries the live editor draft; the text is in a 0600 file, not in argv),
//!  --system-prompt-snapshot off (the draft must be re-sent on resume), --mcp-config <file> + --strict-mcp-config,
//!  --tools "" (no built-in tools), --allowedTools, --disallowedTools, --permission-mode dontAsk + --permission-prompts none,
//!  --setting-sources "" + --disable-slash-commands, --session-id / --resume.
//! The user prompt goes through stdin, so variadic options cannot swallow it.

use super::stream::{ChatStreamEvent, ClaudeRunResult, ClaudeStreamParser};
use super::{MCP_SERVER_NAME, MCP_TOOL_PREFIX};
use crate::core::cli;
use crate::core::error::{AppError, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;
use tokio::time::{sleep_until, Instant};

/// Tools of the app's own MCP server that the chat must never call (syncing to GitHub is a user action).
pub const CHAT_DENIED_TOOLS: [&str; 1] = ["sync_data"];

/// Variables that mark "running inside Claude Code"; passing them on makes a nested `claude` refuse / misbehave.
/// `CLAUDE_CONFIG_DIR` is kept on purpose: it is where the login lives.
pub const CLAUDE_NESTING_ENV: [&str; 9] = [
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_PID",
];

/// SIGTERM → SIGKILL grace period.
pub const KILL_GRACE: Duration = Duration::from_secs(5);
const STDERR_LIMIT: usize = 8_000;

/// How the CLI reaches the app's MCP server: this very binary with the `mcp` subcommand.
#[derive(Debug, Clone)]
pub struct McpLaunch {
    pub exe: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

/// `--mcp-config` content (written to a 0600 file; contains the app paths).
pub fn build_mcp_config(m: &McpLaunch) -> Value {
    json!({
        "mcpServers": {
            MCP_SERVER_NAME: {
                "type": "stdio",
                "command": m.exe.to_string_lossy(),
                "args": ["mcp", "--config-dir", m.config_dir.to_string_lossy(), "--data-dir", m.data_dir.to_string_lossy()]
            }
        }
    })
}

#[derive(Debug, Clone)]
pub struct ClaudeArgsInput<'a> {
    pub model: &'a str,
    pub system_prompt_file: &'a Path,
    pub mcp_config_file: &'a Path,
    /// Names (without the mcp__phrapedia__ prefix) of the MCP tools the chat may use
    pub tool_names: &'a [String],
    /// First turn: new session id (uuid v4)
    pub session_id: Option<&'a str>,
    /// Later turns: session to resume
    pub resume: Option<&'a str>,
    /// settings.ai.args
    pub extra_args: &'a [String],
}

pub fn build_claude_args(i: &ClaudeArgsInput) -> Vec<String> {
    let allowed: Vec<String> = i.tool_names.iter().filter(|n| !CHAT_DENIED_TOOLS.contains(&n.as_str())).map(|n| format!("{MCP_TOOL_PREFIX}{n}")).collect();
    let denied: Vec<String> = CHAT_DENIED_TOOLS.iter().map(|n| format!("{MCP_TOOL_PREFIX}{n}")).collect();
    let s = |x: &str| x.to_string();
    let mut args = vec![s("-p")];
    args.extend(i.extra_args.iter().cloned());
    args.extend([
        s("--output-format"),
        s("stream-json"),
        s("--verbose"),
        s("--include-partial-messages"),
        s("--model"),
        i.model.to_string(),
        s("--system-prompt-file"),
        i.system_prompt_file.to_string_lossy().into_owned(),
        s("--system-prompt-snapshot"),
        s("off"),
        s("--mcp-config"),
        i.mcp_config_file.to_string_lossy().into_owned(),
        s("--strict-mcp-config"),
        s("--tools"),
        s(""),
        s("--allowedTools"),
        allowed.join(","),
        s("--disallowedTools"),
        denied.join(","),
        s("--permission-mode"),
        s("dontAsk"),
        s("--permission-prompts"),
        s("none"),
        // Keep the user's own Claude Code setup (hooks, plugins, skills, output-style rules) out of the app's chat.
        s("--setting-sources"),
        s(""),
        s("--disable-slash-commands"),
    ]);
    if let Some(r) = i.resume {
        args.extend([s("--resume"), r.to_string()]);
    } else if let Some(id) = i.session_id {
        args.extend([s("--session-id"), id.to_string()]);
    }
    args
}

/// Env changes for the child: drop the "inside Claude Code" markers, cap the output tokens. (PATH is set by the runner.)
pub fn claude_env(max_tokens: u32) -> (Vec<(String, String)>, Vec<&'static str>) {
    (vec![("NO_COLOR".into(), "1".into()), ("CLAUDE_CODE_MAX_OUTPUT_TOKENS".into(), max_tokens.to_string())], CLAUDE_NESTING_ENV.to_vec())
}

/// Dedicated, empty working dir for the CLI: it can neither write into the dataset nor pick up project files (CLAUDE.md).
pub fn claude_work_dir(data_dir: &Path) -> Result<PathBuf> {
    let d = data_dir.join("state").join("claude-work");
    std::fs::create_dir_all(&d).map_err(|e| AppError::io("チャット用の作業フォルダを作成できません", e))?;
    Ok(d)
}

/// Private per-run dir (0700) holding the system prompt and MCP config (0600). Removed on drop, also after a crash of the run.
pub struct RunFiles {
    dir: PathBuf,
    pub system_prompt: PathBuf,
    pub mcp_config: PathBuf,
}

fn write_private(path: &Path, content: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path)?.write_all(content.as_bytes())
}

impl RunFiles {
    pub fn create(work_dir: &Path, system_prompt: &str, mcp_config: &Value) -> Result<RunFiles> {
        let dir = work_dir.join(format!("run-{}", uuid::Uuid::new_v4()));
        let mk = || -> std::io::Result<RunFiles> {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
            }
            #[cfg(not(unix))]
            std::fs::create_dir_all(&dir)?;
            let rf = RunFiles { system_prompt: dir.join("system-prompt.txt"), mcp_config: dir.join("mcp.json"), dir: dir.clone() };
            write_private(&rf.system_prompt, system_prompt)?;
            write_private(&rf.mcp_config, &mcp_config.to_string())?;
            Ok(rf)
        };
        mk().map_err(|e| {
            let _ = std::fs::remove_dir_all(&dir);
            AppError::io("チャット用の一時ファイルを作成できません", e)
        })
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

impl Drop for RunFiles {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Remove `run-*` dirs left behind by a hard-killed app (older than a day).
pub fn sweep_stale_runs(work_dir: &Path) {
    let Ok(rd) = std::fs::read_dir(work_dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name();
        if !name.to_string_lossy().starts_with("run-") {
            continue;
        }
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|d| d > Duration::from_secs(86_400));
        if old {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ClaudeProcessResult {
    pub run: ClaudeRunResult,
    pub aborted: bool,
    pub timed_out: bool,
    /// Failed to start the process at all (ENOENT / EACCES ...)
    pub spawn_error: Option<String>,
    pub exit_code: Option<i32>,
    pub stderr: String,
}

pub struct ClaudeProcessOptions<'a> {
    /// Command name or path (resolved through core::cli)
    pub command: &'a str,
    pub args: Vec<String>,
    /// User prompt, written to stdin
    pub prompt: String,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub env_remove: Vec<String>,
    pub timeout: Duration,
    /// Flips to `true` to abort the run
    pub abort: watch::Receiver<bool>,
    /// Override of the grace period between SIGTERM and SIGKILL (tests)
    pub kill_grace: Duration,
}

#[cfg(unix)]
fn killpg(pid: u32, sig: i32) {
    // SAFETY: plain signal syscall on the process group created by `process_group(0)` at spawn time.
    unsafe {
        libc::killpg(pid as i32, sig);
    }
}

#[cfg(not(unix))]
fn killpg(_pid: u32, _sig: i32) {}

#[cfg(unix)]
const SIGTERM: i32 = libc::SIGTERM;
#[cfg(unix)]
const SIGKILL: i32 = libc::SIGKILL;
#[cfg(not(unix))]
const SIGTERM: i32 = 15;
#[cfg(not(unix))]
const SIGKILL: i32 = 9;

/// Spawn `claude -p`, stream its stdout through `ClaudeStreamParser` and return when the process (and its group) is gone.
/// Abort / timeout → SIGTERM to the whole process group, SIGKILL after the grace period. Never fails; problems are in the result.
pub async fn run_claude_process(opts: ClaudeProcessOptions<'_>, on_event: &mut (dyn FnMut(ChatStreamEvent) + Send)) -> ClaudeProcessResult {
    let mut parser = ClaudeStreamParser::new(on_event);
    let mut out = ClaudeProcessResult::default();
    let mut abort = opts.abort.clone();
    if *abort.borrow() {
        out.aborted = true;
        parser.mark_interrupted();
        out.run = parser.result();
        return out;
    }

    let exe = cli::resolve_command(opts.command);
    let mut cmd = Command::new(&exe);
    cmd.args(&opts.args).current_dir(&opts.cwd).env("PATH", cli::child_path());
    for k in &opts.env_remove {
        cmd.env_remove(k);
    }
    for (k, v) in &opts.env {
        cmd.env(k, v);
    }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            out.spawn_error = Some(format!("{e}（{}）", exe.display()));
            parser.mark_interrupted();
            out.run = parser.result();
            return out;
        }
    };
    let pid = child.id().unwrap_or(0);

    if let Some(mut stdin) = child.stdin.take() {
        let prompt = opts.prompt;
        tokio::spawn(async move {
            let _ = stdin.write_all(prompt.as_bytes()).await;
            let _ = stdin.shutdown().await;
        });
    }
    let stderr_task = child.stderr.take().map(|mut e| {
        tokio::spawn(async move {
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            while let Ok(n) = e.read(&mut chunk).await {
                if n == 0 {
                    break;
                }
                if buf.len() < STDERR_LIMIT {
                    buf.extend_from_slice(&chunk[..n.min(STDERR_LIMIT - buf.len())]);
                }
            }
            String::from_utf8_lossy(&buf).into_owned()
        })
    });
    let mut lines = BufReader::new(child.stdout.take().expect("stdout piped")).lines();

    let timeout_at = Instant::now() + opts.timeout;
    let mut kill_at: Option<Instant> = None;
    let mut stdout_done = false;
    let mut abort_open = true;
    let mut exit = None;

    let terminate = |kill_at: &mut Option<Instant>| {
        if kill_at.is_none() {
            killpg(pid, SIGTERM);
            *kill_at = Some(Instant::now() + opts.kill_grace);
        }
    };

    while exit.is_none() {
        tokio::select! {
            line = lines.next_line(), if !stdout_done => match line {
                Ok(Some(l)) => parser.feed(&l),
                _ => stdout_done = true,
            },
            st = child.wait() => exit = Some(st),
            ch = abort.changed(), if abort_open && !out.aborted => match ch {
                Ok(()) if *abort.borrow() => { out.aborted = true; terminate(&mut kill_at); }
                Ok(()) => {}
                Err(_) => abort_open = false,
            },
            _ = sleep_until(timeout_at), if !out.timed_out && !out.aborted => {
                out.timed_out = true;
                terminate(&mut kill_at);
            },
            _ = sleep_until(kill_at.unwrap_or(timeout_at)), if kill_at.is_some() => {
                killpg(pid, SIGKILL);
                kill_at = None;
            },
        }
    }
    // The leader is gone; take down whatever it left in the group (grandchildren holding our pipes), then drain stdout.
    killpg(pid, SIGKILL);
    let drain = async {
        while let Ok(Some(l)) = lines.next_line().await {
            parser.feed(&l);
        }
    };
    let _ = tokio::time::timeout(Duration::from_secs(2), drain).await;

    out.exit_code = exit.and_then(|r| r.ok()).and_then(|s| s.code());
    if let Some(t) = stderr_task {
        out.stderr = tokio::time::timeout(Duration::from_secs(1), t).await.ok().and_then(|r| r.ok()).unwrap_or_default().trim().to_string();
    }
    if out.aborted || out.timed_out || !parser.finished() {
        parser.mark_interrupted();
    }
    out.run = parser.result();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(tools: &'a [String], sp: &'a Path, mc: &'a Path) -> ClaudeArgsInput<'a> {
        ClaudeArgsInput { model: "claude-sonnet-5-5", system_prompt_file: sp, mcp_config_file: mc, tool_names: tools, session_id: None, resume: None, extra_args: &[] }
    }
    fn tool_names() -> Vec<String> {
        ["list_entities", "sync_data", "update_draft", "save_entity"].iter().map(|s| s.to_string()).collect()
    }
    fn value_after<'a>(args: &'a [String], flag: &str) -> &'a str {
        let i = args.iter().position(|a| a == flag).unwrap_or_else(|| panic!("{flag} missing in {args:?}"));
        &args[i + 1]
    }

    #[test]
    fn args_disable_builtin_tools_and_deny_sync_data() {
        let t = tool_names();
        let a = build_claude_args(&input(&t, Path::new("/w/run-1/system-prompt.txt"), Path::new("/w/run-1/mcp.json")));
        assert_eq!(a[0], "-p");
        assert_eq!(value_after(&a, "--tools"), "");
        let allowed = value_after(&a, "--allowedTools");
        assert!(!allowed.contains("sync_data"));
        assert_eq!(allowed, "mcp__phrapedia__list_entities,mcp__phrapedia__update_draft,mcp__phrapedia__save_entity");
        assert_eq!(value_after(&a, "--disallowedTools"), "mcp__phrapedia__sync_data");
        assert_eq!(value_after(&a, "--permission-mode"), "dontAsk");
        assert_eq!(value_after(&a, "--permission-prompts"), "none");
        assert_eq!(value_after(&a, "--setting-sources"), "");
        assert!(a.contains(&"--disable-slash-commands".to_string()));
        assert!(a.contains(&"--strict-mcp-config".to_string()));
        assert_eq!(value_after(&a, "--system-prompt-snapshot"), "off");
        assert_eq!(value_after(&a, "--output-format"), "stream-json");
        assert!(a.contains(&"--verbose".to_string()) && a.contains(&"--include-partial-messages".to_string()));
        assert_eq!(value_after(&a, "--model"), "claude-sonnet-5-5");
    }

    #[test]
    fn files_are_passed_by_path_so_nothing_secret_is_in_argv() {
        let t = tool_names();
        let a = build_claude_args(&input(&t, Path::new("/w/run-1/system-prompt.txt"), Path::new("/w/run-1/mcp.json")));
        assert_eq!(value_after(&a, "--system-prompt-file"), "/w/run-1/system-prompt.txt");
        assert_eq!(value_after(&a, "--mcp-config"), "/w/run-1/mcp.json");
        assert!(!a.iter().any(|x| x == "--system-prompt"));
        assert!(!a.iter().any(|x| x.contains("mcpServers") || x.contains("--config-dir")));
    }

    #[test]
    fn first_turn_uses_session_id_and_later_turns_resume() {
        let t = tool_names();
        let (sp, mc) = (Path::new("a"), Path::new("b"));
        let mut i = input(&t, sp, mc);
        assert!(!build_claude_args(&i).iter().any(|x| x == "--session-id" || x == "--resume"));
        i.session_id = Some("11111111-1111-4111-8111-111111111111");
        let a = build_claude_args(&i);
        assert_eq!(value_after(&a, "--session-id"), "11111111-1111-4111-8111-111111111111");
        assert!(!a.iter().any(|x| x == "--resume"));
        i.resume = Some("22222222-2222-4222-8222-222222222222");
        let a = build_claude_args(&i);
        assert_eq!(value_after(&a, "--resume"), "22222222-2222-4222-8222-222222222222");
        assert!(!a.iter().any(|x| x == "--session-id"));
    }

    #[test]
    fn extra_args_come_right_after_p() {
        let t = tool_names();
        let extra = vec!["--max-turns".to_string(), "5".to_string()];
        let mut i = input(&t, Path::new("a"), Path::new("b"));
        i.extra_args = &extra;
        let a = build_claude_args(&i);
        assert_eq!(&a[..3], ["-p", "--max-turns", "5"]);
    }

    #[test]
    fn mcp_config_points_at_this_binary_with_the_mcp_subcommand() {
        let c = build_mcp_config(&McpLaunch { exe: "/Applications/Phrapedia.app/Contents/MacOS/phrapedia".into(), config_dir: "/c".into(), data_dir: "/d".into() });
        let s = &c["mcpServers"]["phrapedia"];
        assert_eq!(s["type"], "stdio");
        assert_eq!(s["command"], "/Applications/Phrapedia.app/Contents/MacOS/phrapedia");
        assert_eq!(s["args"], json!(["mcp", "--config-dir", "/c", "--data-dir", "/d"]));
    }

    #[test]
    fn env_drops_nesting_markers_but_keeps_the_config_dir() {
        let (set, remove) = claude_env(8192);
        assert!(set.contains(&("CLAUDE_CODE_MAX_OUTPUT_TOKENS".to_string(), "8192".to_string())));
        assert!(remove.contains(&"CLAUDECODE") && remove.contains(&"CLAUDE_CODE_ENTRYPOINT"));
        assert!(!remove.contains(&"CLAUDE_CONFIG_DIR"));
    }

    #[cfg(unix)]
    #[test]
    fn run_files_are_private_and_removed_on_drop() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let rf = RunFiles::create(tmp.path(), "SECRET-PROMPT", &json!({"a": 1})).unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(rf.dir()), 0o700);
        assert_eq!(mode(&rf.system_prompt), 0o600);
        assert_eq!(mode(&rf.mcp_config), 0o600);
        assert_eq!(std::fs::read_to_string(&rf.system_prompt).unwrap(), "SECRET-PROMPT");
        let dir = rf.dir().to_path_buf();
        drop(rf);
        assert!(!dir.exists());
    }

    #[test]
    fn sweep_removes_only_old_run_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("run-new")).unwrap();
        std::fs::create_dir(tmp.path().join("other")).unwrap();
        sweep_stale_runs(tmp.path());
        assert!(tmp.path().join("run-new").exists() && tmp.path().join("other").exists());
    }

    // ---- process control (dummy scripts) ----

    #[cfg(unix)]
    mod process {
        use super::super::*;
        use std::os::unix::fs::PermissionsExt;

        fn script(dir: &Path, body: &str) -> String {
            let p = dir.join("fake-claude.sh");
            std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p.to_string_lossy().into_owned()
        }

        fn opts<'a>(cmd: &'a str, cwd: &Path, timeout: Duration, abort: watch::Receiver<bool>) -> ClaudeProcessOptions<'a> {
            ClaudeProcessOptions { command: cmd, args: vec![], prompt: "hello".into(), cwd: cwd.to_path_buf(), env: vec![], env_remove: vec![], timeout, abort, kill_grace: Duration::from_millis(300) }
        }

        fn alive(pid: i32) -> bool {
            // SAFETY: signal 0 only checks existence.
            unsafe { libc::kill(pid, 0) == 0 }
        }

        async fn wait_for_pid_file(p: &Path) -> i32 {
            for _ in 0..100 {
                if let Ok(s) = std::fs::read_to_string(p) {
                    if let Ok(n) = s.trim().parse() {
                        return n;
                    }
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            panic!("pid file {p:?} not written");
        }

        #[tokio::test]
        async fn streams_events_and_reports_exit() {
            let tmp = tempfile::tempdir().unwrap();
            let cmd = script(
                tmp.path(),
                r#"cat > /dev/null
echo '{"type":"system","subtype":"init","session_id":"sid-1"}'
echo '{"type":"assistant","message":{"id":"m","content":[{"type":"text","text":"こんにちは"}]}}'
echo '{"type":"result","subtype":"success","is_error":false,"result":"ok","session_id":"sid-1"}'
echo warn >&2"#,
            );
            let (_tx, rx) = watch::channel(false);
            let mut events = vec![];
            let r = run_claude_process(opts(&cmd, tmp.path(), Duration::from_secs(10), rx), &mut |e| events.push(e)).await;
            assert_eq!(events, vec![ChatStreamEvent::TextDelta { delta: "こんにちは".into() }]);
            assert!(r.run.finished && !r.aborted && !r.timed_out);
            assert_eq!(r.run.session_id.as_deref(), Some("sid-1"));
            assert_eq!(r.exit_code, Some(0));
            assert_eq!(r.stderr, "warn");
        }

        #[tokio::test]
        async fn prompt_arrives_on_stdin_and_env_is_applied() {
            let tmp = tempfile::tempdir().unwrap();
            let cmd = script(
                tmp.path(),
                r#"P=$(cat)
printf '{"type":"assistant","message":{"id":"m","content":[{"type":"text","text":"%s|%s|%s"}]}}\n' "$P" "$CLAUDE_CODE_MAX_OUTPUT_TOKENS" "${CLAUDECODE:-unset}""#,
            );
            let (_tx, rx) = watch::channel(false);
            let mut o = opts(&cmd, tmp.path(), Duration::from_secs(10), rx);
            o.env = vec![("CLAUDE_CODE_MAX_OUTPUT_TOKENS".into(), "123".into())];
            o.env_remove = vec!["CLAUDECODE".into()];
            std::env::set_var("CLAUDECODE", "1");
            let mut events = vec![];
            run_claude_process(o, &mut |e| events.push(e)).await;
            assert_eq!(events, vec![ChatStreamEvent::TextDelta { delta: "hello|123|unset".into() }]);
        }

        #[tokio::test]
        async fn missing_command_is_a_spawn_error() {
            let tmp = tempfile::tempdir().unwrap();
            let (_tx, rx) = watch::channel(false);
            let r = run_claude_process(opts("/nonexistent/claude-xyz", tmp.path(), Duration::from_secs(5), rx), &mut |_| {}).await;
            assert!(r.spawn_error.is_some());
            assert!(!r.run.finished);
        }

        /// A child that ignores SIGTERM and has a grandchild: abort must reap both (SIGTERM, then SIGKILL on the group).
        #[tokio::test]
        async fn abort_kills_child_and_grandchild_even_when_sigterm_is_ignored() {
            let tmp = tempfile::tempdir().unwrap();
            let pidfile = tmp.path().join("grandchild.pid");
            let cmd = script(
                tmp.path(),
                &format!(
                    r#"trap '' TERM
sh -c 'trap "" TERM; echo $$ > {}; sleep 300' &
echo '{{"type":"assistant","message":{{"id":"m","content":[{{"type":"tool_use","id":"t1","name":"mcp__phrapedia__web_research","input":{{}}}}]}}}}'
while true; do sleep 1; done"#,
                    pidfile.display()
                ),
            );
            let (tx, rx) = watch::channel(false);
            let mut events = vec![];
            let handle = tokio::spawn({
                let cwd = tmp.path().to_path_buf();
                async move {
                    let o = opts(&cmd, &cwd, Duration::from_secs(60), rx);
                    let r = run_claude_process(o, &mut |e| events.push(e)).await;
                    (r, events)
                }
            });
            let grandchild = wait_for_pid_file(&pidfile).await;
            assert!(alive(grandchild));
            let t = std::time::Instant::now();
            tx.send(true).unwrap();
            let (r, events) = handle.await.unwrap();
            assert!(r.aborted && !r.timed_out);
            assert!(t.elapsed() < Duration::from_secs(5), "took {:?}", t.elapsed());
            assert!(matches!(events[0], ChatStreamEvent::ToolStart { .. }));
            // tool without result is marked interrupted
            assert!(matches!(&r.run.parts[0], crate::core::types::ChatPart::Tool { is_error: Some(true), .. }));
            tokio::time::sleep(Duration::from_millis(200)).await;
            assert!(!alive(grandchild), "grandchild {grandchild} survived");
        }

        #[tokio::test]
        async fn timeout_stops_the_group_and_is_reported() {
            let tmp = tempfile::tempdir().unwrap();
            let pidfile = tmp.path().join("g.pid");
            let cmd = script(tmp.path(), &format!("sleep 300 &\necho $! > {}\nwait", pidfile.display()));
            let (_tx, rx) = watch::channel(false);
            let t = std::time::Instant::now();
            let r = run_claude_process(opts(&cmd, tmp.path(), Duration::from_millis(400), rx), &mut |_| {}).await;
            assert!(r.timed_out && !r.aborted);
            assert!(t.elapsed() < Duration::from_secs(5));
            let g = wait_for_pid_file(&pidfile).await;
            tokio::time::sleep(Duration::from_millis(200)).await;
            assert!(!alive(g), "grandchild survived the timeout");
        }

        #[tokio::test]
        async fn grandchild_left_after_normal_exit_is_reaped() {
            let tmp = tempfile::tempdir().unwrap();
            let pidfile = tmp.path().join("g.pid");
            let cmd = script(
                tmp.path(),
                &format!(
                    r#"sleep 300 &
echo $! > {}
echo '{{"type":"result","subtype":"success","is_error":false,"result":"ok","session_id":"s"}}'"#,
                    pidfile.display()
                ),
            );
            let (_tx, rx) = watch::channel(false);
            let r = run_claude_process(opts(&cmd, tmp.path(), Duration::from_secs(10), rx), &mut |_| {}).await;
            assert!(r.run.finished);
            let g = wait_for_pid_file(&pidfile).await;
            tokio::time::sleep(Duration::from_millis(200)).await;
            assert!(!alive(g));
        }

        #[tokio::test]
        async fn already_aborted_never_spawns() {
            let tmp = tempfile::tempdir().unwrap();
            let marker = tmp.path().join("ran");
            let cmd = script(tmp.path(), &format!("touch {}", marker.display()));
            let (tx, rx) = watch::channel(false);
            tx.send(true).unwrap();
            let r = run_claude_process(opts(&cmd, tmp.path(), Duration::from_secs(5), rx), &mut |_| {}).await;
            assert!(r.aborted);
            assert!(!marker.exists());
        }
    }
}
