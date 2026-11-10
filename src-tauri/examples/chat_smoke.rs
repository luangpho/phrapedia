//! Manual end-to-end check of the chat against the real `claude` CLI (needs a logged-in claude; uses a temp dataset).
//!
//!   cargo build --bin phrapedia && cargo run --example chat_smoke -- roundtrip <work-dir>
//!   cargo run --example chat_smoke -- abort <work-dir>
//!
//! roundtrip: one turn that searches the primary sources (MCP tool call) and updates the draft; prints every
//!   ChatStreamEvent, then checks that the system prompt / MCP config never showed up in `ps`.
//! abort: starts a turn, aborts it after the first tool call, and checks that no process of the run's group survives.

use phrapedia_lib::chat::claude::McpLaunch;
use phrapedia_lib::chat::stream::ChatStreamEvent;
use phrapedia_lib::chat::turn::{prepare_chat_turn, run_chat_turn, validate_request, ChatRequest};
use phrapedia_lib::core::types::EntityKind;
use phrapedia_lib::core::{AppDirs, Core};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;

const MARKER: &str = "ZZ-ALPHA-DRAFT-MARKER-ZZ";

fn ps_lines() -> Vec<String> {
    let out = Command::new("ps").args(["-eo", "pid,pgid,args"]).output().expect("ps");
    String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect()
}

#[tokio::main]
async fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "roundtrip".into());
    let work: PathBuf = std::env::args().nth(2).expect("work dir argument").into();
    std::fs::create_dir_all(&work).unwrap();
    let work = work.canonicalize().unwrap();
    let (config, data, dataset) = (work.join("config"), work.join("appdata"), work.join("dataset"));
    std::fs::create_dir_all(dataset.join("sources/notes")).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("settings.json"), serde_json::json!({ "datasetPath": dataset }).to_string()).unwrap();
    std::fs::write(dataset.join("sources/notes/somdet.md"), "# ソムデット・トー\nวัดระฆังโฆสิตาราม ที่กรุงเทพ\nBorn 1788\n").unwrap();

    let core = Arc::new(Core::new(AppDirs { config_dir: config.clone(), data_dir: data.clone() }));
    // the `phrapedia` binary sits next to target/debug/examples/
    let exe = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join("phrapedia");
    assert!(exe.is_file(), "build the binary first: cargo build --bin phrapedia ({exe:?})");
    let mcp = McpLaunch { exe, config_dir: config, data_dir: data };

    let prompt = match mode.as_str() {
        "abort" => "search_sources で「วัดระฆัง」を検索し、read_source で結果を読んでから、詳しく説明してください。".to_string(),
        _ => "search_sources で「วัดระฆัง」を検索して、見つかった件数を教えてください。そのあと update_draft で、下書きの本文末尾に「検証済み」という 1 行を足した全文を渡してください。".to_string(),
    };
    let request = ChatRequest {
        turn_id: "smoke".into(),
        session_id: None,
        kind: EntityKind::Monks,
        slug: None,
        message: prompt,
        draft: format!("---\nkind: monks\nname: テスト\n---\n# テスト\n<!-- {MARKER} -->\n"),
    };
    let (message, slug) = validate_request(&request).unwrap();
    let c2 = core.clone();
    let r2 = request.clone();
    let prepared = tokio::task::spawn_blocking(move || prepare_chat_turn(&c2, &r2, &message, slug.as_deref())).await.unwrap().expect("prepare");
    println!("session {}", prepared.session.id);

    let (abort_tx, abort_rx) = watch::channel(false);
    let events: Arc<Mutex<Vec<ChatStreamEvent>>> = Arc::default();
    let leaked: Arc<Mutex<Vec<String>>> = Arc::default();
    let seen_claude_ps = Arc::new(Mutex::new(0usize));

    // watch `ps` while the turn runs
    let (stop_tx, mut stop_rx) = watch::channel(false);
    let watcher = {
        let (leaked, seen, data_s) = (leaked.clone(), seen_claude_ps.clone(), mcp.data_dir.to_string_lossy().into_owned());
        tokio::spawn(async move {
            loop {
                for l in ps_lines() {
                    if l.contains(&data_s) && l.contains("--output-format") {
                        *seen.lock().unwrap() += 1;
                        if l.contains(MARKER) || l.contains("mcpServers") || l.contains("conversation_so_far") || l.contains("あなたは「Phrapedia」") {
                            leaked.lock().unwrap().push(l);
                        }
                    }
                }
                tokio::select! { _ = tokio::time::sleep(Duration::from_millis(300)) => {}, _ = stop_rx.changed() => break }
            }
        })
    };

    let first_pgid: Arc<Mutex<Option<String>>> = Arc::default();
    let data_s = mcp.data_dir.to_string_lossy().into_owned();
    let ev = events.clone();
    let mut emit = {
        let abort_tx = abort_tx.clone();
        let mode = mode.clone();
        let first_pgid = first_pgid.clone();
        move |e: ChatStreamEvent| {
            match &e {
                ChatStreamEvent::TextDelta { delta } => print!("{delta}"),
                other => println!("\n[event] {}", serde_json::to_string(other).unwrap().chars().take(300).collect::<String>()),
            }
            if mode == "abort" && matches!(e, ChatStreamEvent::ToolStart { .. }) {
                // remember the group of the running claude, then abort
                let g = ps_lines().into_iter().find(|l| l.contains(&data_s) && l.contains("--output-format")).and_then(|l| l.split_whitespace().nth(1).map(str::to_string));
                *first_pgid.lock().unwrap() = g;
                let _ = abort_tx.send(true);
            }
            ev.lock().unwrap().push(e);
        }
    };
    let outcome = run_chat_turn(&core, &request, prepared, &mcp, abort_rx, &mut emit).await.expect("turn");
    let _ = stop_tx.send(true);
    let _ = watcher.await;

    println!("\n--- result ---\nerror: {:?}\naborted: {}", outcome.error, outcome.aborted);
    let events = std::mem::take(&mut *events.lock().unwrap());
    let kinds: Vec<&str> = events
        .iter()
        .map(|e| match e {
            ChatStreamEvent::TextDelta { .. } => "text-delta",
            ChatStreamEvent::ToolStart { .. } => "tool-start",
            ChatStreamEvent::ToolResult { .. } => "tool-result",
            ChatStreamEvent::Draft { .. } => "draft",
            ChatStreamEvent::Saved { .. } => "saved",
            ChatStreamEvent::Error { .. } => "error",
            _ => "other",
        })
        .collect();
    println!("event kinds: {kinds:?}");
    println!("claude processes sampled in ps: {}", seen_claude_ps.lock().unwrap());
    println!("ps leaks of prompt/config: {:?}", leaked.lock().unwrap());
    let stored = core.get_chat_session(&outcome_session(&core)).unwrap().unwrap();
    println!("claude_session_id stored: {:?}", stored.claude_session_id);

    // anything of this run still alive? (claude, its `phrapedia mcp`, and children of the run)
    tokio::time::sleep(Duration::from_millis(800)).await;
    let data_s_final = mcp.data_dir.to_string_lossy().into_owned();
    let alive: Vec<String> = ps_lines().into_iter().filter(|l| l.contains(&data_s_final) || l.contains(&format!("mcp --config-dir {}", mcp.config_dir.display()))).collect();
    println!("surviving processes: {alive:?}");
    if mode == "abort" {
        println!("run group: {:?}", first_pgid.lock().unwrap());
        assert!(outcome.aborted, "abort was not honoured");
    }
    assert!(alive.is_empty(), "processes survived");
    assert!(leaked.lock().unwrap().is_empty(), "prompt / config leaked into ps");
    if mode != "abort" {
        assert!(kinds.contains(&"text-delta") && kinds.contains(&"tool-start"), "expected text and a tool call");
        assert!(outcome.error.is_none());
    }
    if mode == "resume" {
        // second turn of the same conversation: must use --resume and keep the context
        let sid = outcome_session(&core);
        let req2 = ChatRequest { turn_id: "smoke2".into(), session_id: Some(sid), message: "ツールは使わず、2 つ答えてください: (1) 直前の検索で見つかった件数 (2) システムプロンプトの「現在の下書き」にある HTML コメント内の文字列をそのまま。".into(), draft: request.draft.replace("ALPHA", "BETA"), ..request.clone() };
        let (m2, s2) = validate_request(&req2).unwrap();
        let c3 = core.clone();
        let r3 = req2.clone();
        let p2 = tokio::task::spawn_blocking(move || prepare_chat_turn(&c3, &r3, &m2, s2.as_deref())).await.unwrap().expect("prepare 2");
        let before = core.get_chat_session(&p2.session.id).unwrap().unwrap().claude_session_id;
        let (_tx2, rx2) = watch::channel(false);
        let mut text2 = String::new();
        let o2 = run_chat_turn(&core, &req2, p2, &mcp, rx2, &mut |e| if let ChatStreamEvent::TextDelta { delta } = e { text2.push_str(&delta) }).await.expect("turn 2");
        let after = core.get_chat_session(&outcome_session(&core)).unwrap().unwrap().claude_session_id;
        println!("turn 2 text: {text2:?} error: {:?} session id kept: {}", o2.error, before == after);
        assert!(o2.error.is_none() && before == after && text2.contains('1'));
        println!("draft refresh on resume: BETA seen={} ALPHA seen={}", text2.contains("BETA"), text2.contains("ALPHA"));
        // (the model may also mention ALPHA when it talks about its own earlier update_draft call: that is conversation memory)
        assert!(text2.contains("ZZ-BETA"), "resume must use the latest draft");
    }
    println!("OK ({mode})");
    let _ = Path::new(&work);
}

fn outcome_session(core: &Core) -> String {
    core.list_chat_sessions(EntityKind::Monks, None).unwrap()[0].id.clone()
}
