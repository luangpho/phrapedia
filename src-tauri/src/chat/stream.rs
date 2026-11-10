//! Converts the line-delimited JSON of `claude -p --output-format stream-json --verbose --include-partial-messages`
//! into `ChatStreamEvent`s (port of claude-stream.ts, verified against Claude Code 2.1.317).
//!
//! Events used:
//!  - `system/init`                      → session id
//!  - `stream_event` content_block_delta → text-delta (partial messages)
//!  - `assistant` message                → tool_use blocks → tool-start (update_draft → draft); text blocks of messages
//!    that were not streamed as deltas are emitted as text-delta
//!  - `user` message                     → tool_result blocks → tool-result (+ saved for save_entity)
//!  - `result`                           → final status / error

use super::MCP_TOOL_PREFIX;
use crate::core::types::{ChatMessage, ChatPart, EntityKind, ToolState};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub const UPDATE_DRAFT_TOOL: &str = "update_draft";

/// Same limit as the former SDK loop: tool output sent to the UI / stored in history is cut here (in characters).
pub const EVENT_OUTPUT_LIMIT: usize = 20_000;

/// Wire event of `chat_send` (identical to shared/types/chat.ts `ChatStreamEvent`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum ChatStreamEvent {
    Session { session_id: String, user_message_id: String, assistant_message_id: String },
    TextDelta { delta: String },
    ToolStart { tool_call_id: String, name: String, input: Value },
    ToolResult {
        tool_call_id: String,
        name: String,
        output: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    Draft {
        markdown: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    Saved {
        kind: EntityKind,
        slug: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
    Done { message: ChatMessage },
    Error { message: String },
}

fn truncate(text: &str, limit: usize) -> String {
    let n = text.chars().count();
    if n > limit {
        let head: String = text.chars().take(limit).collect();
        format!("{head}\n…({} 文字省略)", n - limit)
    } else {
        text.to_string()
    }
}

/// `mcp__phrapedia__search_sources` → `search_sources` (the UI and the stored history use the short name).
pub fn short_tool_name(name: &str) -> &str {
    name.strip_prefix(MCP_TOOL_PREFIX).unwrap_or(name)
}

/// save_entity returns JSON like `{ ok, kind, slug, path }`; extract what the UI needs.
pub fn parse_saved_result(text: &str) -> Option<(EntityKind, String, Option<String>)> {
    let v: Value = serde_json::from_str(text).ok()?;
    if v.get("ok").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let slug = v.get("slug")?.as_str()?.to_string();
    let kind = EntityKind::parse(v.get("kind")?.as_str()?)?;
    Some((kind, slug, v.get("path").and_then(Value::as_str).map(str::to_string)))
}

/// tool_result content is a string or an array of `{type:'text'|'image'}` blocks.
pub fn flatten_tool_result(content: &Value) -> (String, usize) {
    match content {
        Value::String(s) => (s.clone(), 0),
        Value::Array(items) => {
            let mut texts = vec![];
            let mut images = 0;
            for c in items {
                match c.get("type").and_then(Value::as_str) {
                    Some("text") if c.get("text").is_some_and(Value::is_string) => texts.push(c["text"].as_str().unwrap_or_default().to_string()),
                    Some("image") => images += 1,
                    Some(other) => texts.push(format!("[{other} content]")),
                    None => {}
                }
            }
            (texts.join("\n"), images)
        }
        Value::Null => (String::new(), 0),
        other => (other.to_string(), 0),
    }
}

#[derive(Debug, Clone, Default)]
pub struct ClaudeRunResult {
    /// Assistant parts in order (text, tool, draft)
    pub parts: Vec<ChatPart>,
    /// Session id reported by the CLI (system/init or result)
    pub session_id: Option<String>,
    /// The CLI's `result` message was seen
    pub finished: bool,
    /// Error text from an is_error result (not yet turned into a user-facing message)
    pub result_error: Option<String>,
    /// subtype of the result message (success | error_during_execution | error_max_turns ...)
    pub result_subtype: Option<String>,
}

pub struct ClaudeStreamParser<'a> {
    on_event: &'a mut (dyn FnMut(ChatStreamEvent) + Send),
    parts: Vec<ChatPart>,
    session_id: Option<String>,
    finished: bool,
    result_error: Option<String>,
    result_subtype: Option<String>,
    streamed_messages: HashSet<String>,
    current_message_id: Option<String>,
    seen_tool_uses: HashSet<String>,
    /// tool_use id → short tool name
    tool_names: HashMap<String, String>,
    /// tool_use id → index into `parts`
    tool_parts: HashMap<String, usize>,
    hidden_tool_uses: HashSet<String>,
}

impl<'a> ClaudeStreamParser<'a> {
    pub fn new(on_event: &'a mut (dyn FnMut(ChatStreamEvent) + Send)) -> Self {
        ClaudeStreamParser {
            on_event,
            parts: vec![],
            session_id: None,
            finished: false,
            result_error: None,
            result_subtype: None,
            streamed_messages: HashSet::new(),
            current_message_id: None,
            seen_tool_uses: HashSet::new(),
            tool_names: HashMap::new(),
            tool_parts: HashMap::new(),
            hidden_tool_uses: HashSet::new(),
        }
    }

    pub fn finished(&self) -> bool {
        self.finished
    }

    pub fn result(&self) -> ClaudeRunResult {
        ClaudeRunResult {
            parts: self.parts.clone(),
            session_id: self.session_id.clone(),
            finished: self.finished,
            result_error: self.result_error.clone(),
            result_subtype: self.result_subtype.clone(),
        }
    }

    /// Mark tool parts that never got a result (killed / aborted run).
    pub fn mark_interrupted(&mut self) {
        for p in &mut self.parts {
            if let ChatPart::Tool { state, is_error, output, .. } = p {
                if *state == ToolState::Running {
                    *state = ToolState::Done;
                    *is_error = Some(true);
                    if output.is_none() {
                        *output = Some(Value::String("（中断されました）".into()));
                    }
                }
            }
        }
    }

    pub fn feed(&mut self, line: &str) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return;
        }
        let Ok(msg) = serde_json::from_str::<Value>(trimmed) else { return }; // non-JSON noise on stdout
        let Some(obj) = msg.as_object() else { return };
        // Sub-agent traffic is not part of the conversation shown to the user.
        if obj.get("parent_tool_use_id").is_some_and(|v| !v.is_null() && v.as_str() != Some("")) {
            return;
        }
        match obj.get("type").and_then(Value::as_str) {
            Some("system") => {
                if obj.get("subtype").and_then(Value::as_str) == Some("init") {
                    if let Some(s) = obj.get("session_id").and_then(Value::as_str) {
                        self.session_id = Some(s.to_string());
                    }
                }
            }
            Some("stream_event") => self.on_stream_event(obj.get("event")),
            Some("assistant") => self.on_assistant(obj.get("message")),
            Some("user") => self.on_user(obj.get("message")),
            Some("result") => self.on_result(&msg),
            _ => {}
        }
    }

    fn append_text(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        match self.parts.last_mut() {
            Some(ChatPart::Text { text }) => text.push_str(delta),
            _ => self.parts.push(ChatPart::Text { text: delta.to_string() }),
        }
        (self.on_event)(ChatStreamEvent::TextDelta { delta: delta.to_string() });
    }

    fn on_stream_event(&mut self, ev: Option<&Value>) {
        let Some(ev) = ev else { return };
        match ev.get("type").and_then(Value::as_str) {
            Some("message_start") => {
                self.current_message_id = ev.pointer("/message/id").and_then(Value::as_str).map(str::to_string);
            }
            Some("content_block_delta") if ev.pointer("/delta/type").and_then(Value::as_str) == Some("text_delta") => {
                if let Some(t) = ev.pointer("/delta/text").and_then(Value::as_str) {
                    if let Some(id) = &self.current_message_id {
                        self.streamed_messages.insert(id.clone());
                    }
                    self.append_text(t);
                }
            }
            _ => {}
        }
    }

    fn on_assistant(&mut self, message: Option<&Value>) {
        let Some(message) = message else { return };
        let Some(content) = message.get("content").and_then(Value::as_array) else { return };
        let streamed = message.get("id").and_then(Value::as_str).is_some_and(|id| self.streamed_messages.contains(id));
        for block in content {
            match block.get("type").and_then(Value::as_str) {
                // not streamed as deltas (partial messages off / synthetic message): emit it whole
                Some("text") if !streamed => {
                    if let Some(t) = block.get("text").and_then(Value::as_str) {
                        self.append_text(t);
                    }
                }
                Some("tool_use") => self.on_tool_use(block),
                _ => {}
            }
        }
    }

    fn on_tool_use(&mut self, block: &Value) {
        let id = block.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
        if id.is_empty() || !self.seen_tool_uses.insert(id.clone()) {
            return;
        }
        let name = short_tool_name(block.get("name").and_then(Value::as_str).unwrap_or_default()).to_string();
        let input = match block.get("input") {
            Some(v) if v.is_object() => v.clone(),
            _ => Value::Object(Default::default()),
        };
        self.tool_names.insert(id.clone(), name.clone());

        if name == UPDATE_DRAFT_TOOL {
            self.hidden_tool_uses.insert(id);
            let markdown = input.get("markdown").and_then(Value::as_str).unwrap_or_default().to_string();
            let summary = input.get("summary").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
            if !markdown.trim().is_empty() {
                self.parts.push(ChatPart::Draft { markdown: markdown.clone(), summary: summary.clone() });
                (self.on_event)(ChatStreamEvent::Draft { markdown, summary });
            }
            return;
        }

        self.parts.push(ChatPart::Tool { tool_call_id: id.clone(), name: name.clone(), input: input.clone(), output: None, is_error: None, state: ToolState::Running });
        self.tool_parts.insert(id.clone(), self.parts.len() - 1);
        (self.on_event)(ChatStreamEvent::ToolStart { tool_call_id: id, name, input });
    }

    fn on_user(&mut self, message: Option<&Value>) {
        let Some(content) = message.and_then(|m| m.get("content")).and_then(Value::as_array) else { return };
        for block in content {
            if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            let id = block.get("tool_use_id").and_then(Value::as_str).unwrap_or_default().to_string();
            if id.is_empty() || self.hidden_tool_uses.contains(&id) {
                continue;
            }
            let name = self.tool_names.get(&id).cloned().unwrap_or_else(|| "unknown".into());
            let (text, images) = flatten_tool_result(block.get("content").unwrap_or(&Value::Null));
            let is_error = block.get("is_error").and_then(Value::as_bool).unwrap_or(false);
            let mut output = truncate(&text, EVENT_OUTPUT_LIMIT);
            if images > 0 {
                output.push_str(&format!("\n[画像 {images} 件]"));
            }
            let flag = is_error.then_some(true);
            if let Some(&idx) = self.tool_parts.get(&id) {
                if let ChatPart::Tool { output: o, is_error: e, state, .. } = &mut self.parts[idx] {
                    *o = Some(Value::String(output.clone()));
                    *e = flag;
                    *state = ToolState::Done;
                }
            }
            (self.on_event)(ChatStreamEvent::ToolResult { tool_call_id: id, name: name.clone(), output: Value::String(output), is_error: flag });
            if name == "save_entity" && !is_error {
                if let Some((kind, slug, path)) = parse_saved_result(&text) {
                    (self.on_event)(ChatStreamEvent::Saved { kind, slug, path });
                }
            }
        }
    }

    fn on_result(&mut self, msg: &Value) {
        self.finished = true;
        if let Some(s) = msg.get("session_id").and_then(Value::as_str) {
            self.session_id = Some(s.to_string());
        }
        self.result_subtype = msg.get("subtype").and_then(Value::as_str).map(str::to_string);
        let is_error = msg.get("is_error").and_then(Value::as_bool).unwrap_or(false);
        if is_error || self.result_subtype.as_deref().is_some_and(|s| s != "success") {
            let text = match msg.get("result").and_then(Value::as_str).map(str::trim) {
                Some(t) if !t.is_empty() => t.to_string(),
                _ => match msg.get("errors").and_then(Value::as_array) {
                    Some(errs) => errs.iter().map(|e| e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string())).collect::<Vec<_>>().join("; "),
                    None => self.result_subtype.clone().unwrap_or_else(|| "error".into()),
                },
            };
            self.result_error = Some(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SID: &str = "5c77479f-23ba-41cb-b2bd-99046818ffb0";

    fn line(v: Value) -> String {
        v.to_string()
    }
    fn text_delta(msg_id: &str, text: &str) -> String {
        line(json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}},"session_id":SID,"parent_tool_use_id":null,"api_message_id":msg_id}))
    }
    fn message_start(id: &str) -> String {
        line(json!({"type":"stream_event","event":{"type":"message_start","message":{"id":id,"role":"assistant","content":[]}},"session_id":SID,"parent_tool_use_id":null}))
    }
    fn assistant(id: &str, content: Value) -> String {
        line(json!({"type":"assistant","message":{"id":id,"role":"assistant","content":content},"session_id":SID,"parent_tool_use_id":null}))
    }
    fn tool_result(id: &str, content: Value, is_error: bool) -> String {
        line(json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":id,"content":content,"is_error":is_error}]},"session_id":SID,"parent_tool_use_id":null}))
    }

    fn run(lines: &[String]) -> (Vec<ChatStreamEvent>, ClaudeRunResult, bool) {
        let mut events = vec![];
        let (result, finished) = {
            let mut sink = |e| events.push(e);
            let mut p = ClaudeStreamParser::new(&mut sink);
            for l in lines {
                p.feed(l);
            }
            (p.result(), p.finished())
        };
        (events, result, finished)
    }

    /// system/init → text delta → tool_use → tool_result → text delta → result (shape of Claude Code 2.1.317)
    fn fixture() -> Vec<String> {
        vec![
            line(json!({"type":"system","subtype":"init","session_id":SID,"tools":[],"mcp_servers":[{"name":"phrapedia","status":"connected"}],"model":"claude-sonnet-5-5"})),
            line(json!({"type":"system","subtype":"status","status":"requesting","session_id":SID})),
            message_start("msg_1"),
            text_delta("msg_1", "調べます"),
            text_delta("msg_1", "。"),
            assistant("msg_1", json!([{"type":"text","text":"調べます。"},{"type":"tool_use","id":"toolu_1","name":"mcp__phrapedia__search_sources","input":{"query":"ソムデット"}}])),
            tool_result("toolu_1", json!([{"type":"text","text":"{\"count\":2}"}]), false),
            message_start("msg_2"),
            text_delta("msg_2", "2 件でした。"),
            assistant("msg_2", json!([{"type":"text","text":"2 件でした。"}])),
            line(json!({"type":"rate_limit_event","rate_limit_info":{"status":"allowed"}})),
            line(json!({"type":"result","subtype":"success","is_error":false,"result":"2 件でした。","session_id":SID})),
        ]
    }

    #[test]
    fn converts_a_full_turn_into_events_in_order() {
        let (events, res, finished) = run(&fixture());
        assert_eq!(
            events,
            vec![
                ChatStreamEvent::TextDelta { delta: "調べます".into() },
                ChatStreamEvent::TextDelta { delta: "。".into() },
                ChatStreamEvent::ToolStart { tool_call_id: "toolu_1".into(), name: "search_sources".into(), input: json!({"query":"ソムデット"}) },
                ChatStreamEvent::ToolResult { tool_call_id: "toolu_1".into(), name: "search_sources".into(), output: json!("{\"count\":2}"), is_error: None },
                ChatStreamEvent::TextDelta { delta: "2 件でした。".into() },
            ]
        );
        assert!(finished && res.finished);
        assert_eq!(res.session_id.as_deref(), Some(SID));
        assert!(res.result_error.is_none());
        assert_eq!(
            res.parts,
            vec![
                ChatPart::Text { text: "調べます。".into() },
                ChatPart::Tool {
                    tool_call_id: "toolu_1".into(),
                    name: "search_sources".into(),
                    input: json!({"query":"ソムデット"}),
                    output: Some(json!("{\"count\":2}")),
                    is_error: None,
                    state: ToolState::Done
                },
                ChatPart::Text { text: "2 件でした。".into() },
            ]
        );
    }

    #[test]
    fn does_not_duplicate_text_already_streamed_as_deltas() {
        let (events, ..) = run(&fixture());
        let all: String = events.iter().filter_map(|e| if let ChatStreamEvent::TextDelta { delta } = e { Some(delta.as_str()) } else { None }).collect();
        assert_eq!(all, "調べます。2 件でした。");
    }

    #[test]
    fn emits_text_of_assistant_messages_that_were_not_streamed() {
        let (events, res, _) = run(&[assistant("msg_x", json!([{"type":"text","text":"そのまま"}]))]);
        assert_eq!(events, vec![ChatStreamEvent::TextDelta { delta: "そのまま".into() }]);
        assert_eq!(res.parts, vec![ChatPart::Text { text: "そのまま".into() }]);
    }

    #[test]
    fn maps_tool_errors_and_truncates_long_outputs() {
        let big = "x".repeat(EVENT_OUTPUT_LIMIT + 50);
        let (events, ..) = run(&[
            assistant("m", json!([{"type":"tool_use","id":"t1","name":"mcp__phrapedia__read_source","input":{"path":"a.md"}}])),
            tool_result("t1", json!(big), true),
        ]);
        let ChatStreamEvent::ToolResult { output, is_error, .. } = &events[1] else { panic!("{events:?}") };
        assert_eq!(*is_error, Some(true));
        let s = output.as_str().unwrap();
        assert!(s.contains("…(50 文字省略)"));
        assert!(s.chars().count() < EVENT_OUTPUT_LIMIT + 40);
    }

    #[test]
    fn truncation_counts_characters_not_bytes() {
        let text = "あ".repeat(EVENT_OUTPUT_LIMIT + 3);
        let out = truncate(&text, EVENT_OUTPUT_LIMIT);
        assert!(out.starts_with(&"あ".repeat(EVENT_OUTPUT_LIMIT)));
        assert!(out.ends_with("…(3 文字省略)"));
        assert_eq!(truncate("short", EVENT_OUTPUT_LIMIT), "short");
    }

    #[test]
    fn notes_images_in_tool_results() {
        assert_eq!(flatten_tool_result(&json!([{"type":"text","text":"a"},{"type":"image","source":{}}])), ("a".to_string(), 1));
        let (events, ..) = run(&[
            assistant("m", json!([{"type":"tool_use","id":"t1","name":"mcp__phrapedia__read_source","input":{}}])),
            tool_result("t1", json!([{"type":"image","source":{}}]), false),
        ]);
        let ChatStreamEvent::ToolResult { output, .. } = &events[1] else { panic!() };
        assert!(output.as_str().unwrap().contains("[画像 1 件]"));
    }

    #[test]
    fn emits_saved_after_a_successful_save_entity() {
        let (events, ..) = run(&[
            assistant("m", json!([{"type":"tool_use","id":"s1","name":"mcp__phrapedia__save_entity","input":{"kind":"monks"}}])),
            tool_result("s1", json!(json!({"ok":true,"kind":"monks","slug":"somdet-to","path":"data/monks/somdet-to.md"}).to_string()), false),
        ]);
        let kinds: Vec<_> = events.iter().map(|e| serde_json::to_value(e).unwrap()["type"].as_str().unwrap().to_string()).collect();
        assert_eq!(kinds, ["tool-start", "tool-result", "saved"]);
        assert_eq!(events[2], ChatStreamEvent::Saved { kind: EntityKind::Monks, slug: "somdet-to".into(), path: Some("data/monks/somdet-to.md".into()) });
    }

    #[test]
    fn no_saved_when_save_entity_failed_or_result_is_not_ok() {
        let (events, ..) = run(&[
            assistant("m", json!([{"type":"tool_use","id":"s1","name":"mcp__phrapedia__save_entity","input":{}}])),
            tool_result("s1", json!(json!({"ok":true,"kind":"monks","slug":"x"}).to_string()), true),
        ]);
        assert_eq!(events.len(), 2);
        assert_eq!(parse_saved_result(r#"{"ok":false,"kind":"monks","slug":"x"}"#), None);
        assert_eq!(parse_saved_result(r#"{"ok":true,"kind":"nope","slug":"x"}"#), None);
        assert_eq!(parse_saved_result("not json"), None);
    }

    #[test]
    fn reports_an_error_result_and_ignores_noise_and_sub_agent_traffic() {
        let (events, res, _) = run(&[
            "not json at all".into(),
            "".into(),
            line(json!({"type":"assistant","parent_tool_use_id":"toolu_parent","message":{"id":"sub","content":[{"type":"text","text":"sub-agent"}]}})),
            line(json!({"type":"result","subtype":"error_during_execution","is_error":true,"result":"No conversation found with session ID: x","session_id":"x"})),
        ]);
        assert!(events.is_empty());
        assert!(res.finished);
        assert_eq!(res.result_subtype.as_deref(), Some("error_during_execution"));
        assert_eq!(res.result_error.as_deref(), Some("No conversation found with session ID: x"));
        assert_eq!(res.session_id.as_deref(), Some("x"));
    }

    #[test]
    fn error_result_falls_back_to_errors_array_then_subtype() {
        let (_, r, _) = run(&[line(json!({"type":"result","subtype":"error_max_turns","is_error":true,"errors":["a","b"]}))]);
        assert_eq!(r.result_error.as_deref(), Some("a; b"));
        let (_, r, _) = run(&[line(json!({"type":"result","subtype":"error_max_turns"}))]);
        assert_eq!(r.result_error.as_deref(), Some("error_max_turns"));
    }

    #[test]
    fn marks_tools_without_a_result_as_interrupted() {
        let mut sink = |_| {};
        let mut p = ClaudeStreamParser::new(&mut sink);
        p.feed(&assistant("m", json!([{"type":"tool_use","id":"t1","name":"mcp__phrapedia__web_research","input":{"query":"q"}}])));
        p.mark_interrupted();
        let ChatPart::Tool { state, is_error, output, .. } = &p.result().parts[0] else { panic!() };
        assert_eq!((*state, *is_error, output.clone()), (ToolState::Done, Some(true), Some(json!("（中断されました）"))));
    }

    #[test]
    fn strips_the_mcp_prefix_only_from_phrapedia_tools() {
        assert_eq!(short_tool_name("mcp__phrapedia__get_entity"), "get_entity");
        assert_eq!(short_tool_name("mcp__other__get_entity"), "mcp__other__get_entity");
    }

    fn draft_lines(input: Value) -> Vec<String> {
        vec![
            assistant("m", json!([{"type":"tool_use","id":"d1","name":"mcp__phrapedia__update_draft","input":input}])),
            tool_result("d1", json!("draft updated (not saved yet)"), false),
        ]
    }

    #[test]
    fn update_draft_becomes_a_draft_event_and_hides_the_tool_call() {
        let (events, res, _) = run(&draft_lines(json!({"markdown":"---\nkind: monks\n---\n# 本文","summary":"  見出しを追加  "})));
        let want = ChatStreamEvent::Draft { markdown: "---\nkind: monks\n---\n# 本文".into(), summary: Some("見出しを追加".into()) };
        assert_eq!(events, vec![want]);
        assert_eq!(res.parts, vec![ChatPart::Draft { markdown: "---\nkind: monks\n---\n# 本文".into(), summary: Some("見出しを追加".into()) }]);
    }

    #[test]
    fn update_draft_omits_empty_summary_and_ignores_empty_markdown() {
        assert_eq!(run(&draft_lines(json!({"markdown":"# a"}))).0, vec![ChatStreamEvent::Draft { markdown: "# a".into(), summary: None }]);
        assert!(run(&draft_lines(json!({"markdown":"   "}))).0.is_empty());
    }

    #[test]
    fn emits_one_draft_per_tool_use_id() {
        let l = assistant("m", json!([{"type":"tool_use","id":"d1","name":"mcp__phrapedia__update_draft","input":{"markdown":"# a"}}]));
        assert_eq!(run(&[l.clone(), l]).0.len(), 1);
    }

    #[test]
    fn wire_format_matches_the_ts_contract() {
        let ev = serde_json::to_value(ChatStreamEvent::Session { session_id: "s".into(), user_message_id: "u".into(), assistant_message_id: "a".into() }).unwrap();
        assert_eq!(ev, json!({"type":"session","sessionId":"s","userMessageId":"u","assistantMessageId":"a"}));
        let ev = serde_json::to_value(ChatStreamEvent::ToolStart { tool_call_id: "t".into(), name: "n".into(), input: json!({}) }).unwrap();
        assert_eq!(ev, json!({"type":"tool-start","toolCallId":"t","name":"n","input":{}}));
        let ev = serde_json::to_value(ChatStreamEvent::TextDelta { delta: "x".into() }).unwrap();
        assert_eq!(ev, json!({"type":"text-delta","delta":"x"}));
        let ev = serde_json::to_value(ChatStreamEvent::Saved { kind: EntityKind::Temples, slug: "s".into(), path: None }).unwrap();
        assert_eq!(ev, json!({"type":"saved","kind":"temples","slug":"s"}));
        let ev = serde_json::to_value(ChatStreamEvent::Error { message: "m".into() }).unwrap();
        assert_eq!(ev, json!({"type":"error","message":"m"}));
    }
}
