//! The MCP tools, resource and prompt of the app (port of server/mcp/**). Pure functions over `Core`, independent of
//! rmcp so they are unit-testable; `server.rs` adapts them to the protocol. Names, descriptions (Japanese kept) and
//! input schemas follow the former Nitro implementation.

use crate::core::entities::{assert_kind, assert_slug, entity_template};
use crate::core::error::AppError;
use crate::core::frontmatter;
use crate::core::types::{EntityKind, SourceFileKind};
use crate::core::Core;
use serde::Serialize;
use serde_json::{json, Map, Value};

pub const TOOL_NAMES: [&str; 11] = [
    "list_entities",
    "get_entity",
    "entity_template",
    "save_entity",
    "list_sources",
    "read_source",
    "search_sources",
    "web_research",
    "sync_status",
    "sync_data",
    "update_draft",
];

/// Returned by `update_draft`; the in-app chat runner turns the *call* into a `draft` event, the tool itself changes nothing.
pub const UPDATE_DRAFT_ACK: &str = "draft updated (not saved yet)";

pub struct ToolSpec {
    pub name: &'static str,
    pub title: &'static str,
    pub description: String,
    pub schema: Value,
    pub read_only: bool,
    pub destructive: Option<bool>,
    pub idempotent: Option<bool>,
    pub open_world: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Text(String),
    Image { data: String, mime_type: String },
}

/// Ok = tool result content; Err = message shown to the model with `isError: true`.
pub type ToolResult = std::result::Result<Vec<Content>, String>;

fn kind_schema(desc: Option<&str>) -> Value {
    let mut o = json!({"type": "string", "enum": ["monks", "temples", "amulets"]});
    if let Some(d) = desc {
        o["description"] = json!(d);
    }
    o
}

fn schema(props: Value, required: &[&str]) -> Value {
    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": props,
        "required": required,
        "additionalProperties": false
    })
}

pub fn specs() -> Vec<ToolSpec> {
    let spec = |name, title, description: &str, schema, read_only, destructive, idempotent, open_world| ToolSpec {
        name,
        title,
        description: description.to_string(),
        schema,
        read_only,
        destructive,
        idempotent,
        open_world,
    };
    vec![
        spec(
            "list_entities",
            "List records",
            "List saved records (monks / temples / amulets) with slug, name and status. Use this to find the slug of an existing record before reading or updating it.",
            schema(json!({"kind": kind_schema(Some("Record kind"))}), &["kind"]),
            true,
            None,
            None,
            None,
        ),
        spec(
            "get_entity",
            "Read a record",
            "Return the full markdown (YAML frontmatter + body) of one record.",
            schema(json!({"kind": kind_schema(None), "slug": {"type": "string", "description": "Record slug (file name without .md)"}}), &["kind", "slug"]),
            true,
            None,
            None,
            None,
        ),
        spec(
            "entity_template",
            "Record template",
            "Return the starter markdown (frontmatter + section headings) for a new record of the given kind.",
            schema(json!({"kind": kind_schema(None), "name": {"type": "string", "description": "Display name to pre-fill"}}), &["kind"]),
            true,
            None,
            None,
            None,
        ),
        spec(
            "save_entity",
            "Create or update a record",
            &[
                "Write a record as a markdown file under data/<kind>/<slug>.md. Pass the FULL markdown including the YAML frontmatter",
                "(slug, kind, name, name_th, name_en, aliases, temple, province, born/died or founded, status, sources, tags).",
                "Omit slug to create a new record (the slug is derived from name_en / name). Existing files are overwritten, so read first with get_entity when updating.",
            ]
            .join(" "),
            schema(
                json!({
                    "kind": kind_schema(None),
                    "slug": {"type": "string", "description": "Existing slug to update, or omit to create"},
                    "markdown": {"type": "string", "description": "Full markdown with frontmatter"},
                    "message": {"type": "string", "description": "Commit message hint (used when auto-commit is enabled)"}
                }),
                &["kind", "markdown"],
            ),
            false,
            Some(true),
            Some(true),
            None,
        ),
        spec(
            "list_sources",
            "List primary sources",
            "List files in the primary-sources directory (markdown notes, images, PDFs). Primary sources are authoritative; prefer them over web research.",
            schema(json!({"subdir": {"type": "string", "description": "Sub-directory to list (relative to the sources directory)"}}), &[]),
            true,
            None,
            None,
            None,
        ),
        spec(
            "read_source",
            "Read a primary source",
            "Read a file from the primary-sources directory. Markdown/text is returned as text; images are returned as image content so they can be inspected.",
            schema(
                json!({
                    "path": {"type": "string", "description": "Path relative to the sources directory, as returned by list_sources / search_sources"},
                    "maxChars": {"type": "integer", "minimum": 100, "maximum": 200000, "description": "Truncate text after this many characters (default 40000)"}
                }),
                &["path"],
            ),
            true,
            None,
            None,
            None,
        ),
        spec(
            "search_sources",
            "Search primary sources",
            "Full-text (substring, case-insensitive) search over markdown/text files in the primary-sources directory. Thai, Japanese and English terms all work. Returns paths with a snippet; follow up with read_source.",
            schema(
                json!({
                    "query": {"type": "string", "description": "Search terms separated by spaces (name in Thai / Japanese / English, temple, province, ...)"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 50, "description": "Max hits (default 20)"}
                }),
                &["query"],
            ),
            true,
            None,
            None,
            None,
        ),
        spec(
            "web_research",
            "Web research via Google CLI (Antigravity / Gemini)",
            &[
                "Research a question on the web using the configured Google CLI (Antigravity CLI `agy` by default, or the legacy Gemini CLI) with Google Search. Use ONLY for information that is missing from or unclear in the primary sources.",
                "When the user's request is about an IMAGE from the primary sources (\"この画像の…\"), you MUST pass the image path(s) in `images` and the text you yourself read from the image in `ocrText`: the CLI then opens the image, reads its text on its own (re-reading enlarged parts when a match is not certain), compares it with your reading, and searches with the extracted strings.",
                "The answer is markdown with source URLs; everything it returns must be treated as UNVERIFIED and labelled as such in records.",
                "Results are cached per query (and per image contents).",
            ]
            .join(" "),
            schema(
                json!({
                    "query": {"type": "string", "description": "What to find out (be specific: name in Thai, temple, province, period)"},
                    "context": {"type": "string", "description": "Known facts from primary sources, so the web search complements rather than contradicts them"},
                    "images": {"type": "array", "items": {"type": "string"}, "maxItems": 8, "description": "Primary-source image paths (relative to the sources dir, as returned by list_sources) that the question is about. The images themselves are handed to the CLI."},
                    "ocrText": {"type": "string", "description": "Text you (the assistant) read from those images, as reference for the CLI. It will re-read the image itself and report differences."},
                    "noCache": {"type": "boolean", "description": "Skip the research cache"}
                }),
                &["query"],
            ),
            true,
            None,
            None,
            Some(true),
        ),
        spec(
            "sync_status",
            "Git sync status",
            "Show the git status of the data directory: branch, remote, uncommitted record changes, ahead/behind counts.",
            schema(json!({}), &[]),
            true,
            None,
            None,
            None,
        ),
        spec(
            "sync_data",
            "Commit and sync records with GitHub",
            "Commit all changes under the data directory, pull --rebase from the configured remote branch, then push. Only the data directory is staged.",
            schema(
                json!({
                    "message": {"type": "string", "description": "Commit message"},
                    "mode": {"type": "string", "enum": ["commit", "pull", "push", "sync"], "description": "Default: sync (commit + pull + push)"}
                }),
                &[],
            ),
            false,
            Some(false),
            None,
            Some(true),
        ),
        spec(
            "update_draft",
            "Update the editor draft (in-app chat only)",
            &[
                "For the Phrapedia in-app chat ONLY: replace the working draft shown in the user's editor with new content.",
                "Pass the FULL updated markdown (YAML frontmatter + body), never a diff or a fragment.",
                "This does NOT save any file; the user saves it. Use it whenever you add or change record content.",
                "Calling it from an external MCP client has no effect (it only returns an acknowledgement).",
            ]
            .join(" "),
            schema(
                json!({
                    "markdown": {"type": "string", "minLength": 1, "description": "Full markdown of the record including the YAML frontmatter"},
                    "summary": {"type": "string", "description": "One-line summary (Japanese) of what changed"}
                }),
                &["markdown"],
            ),
            true,
            Some(false),
            Some(true),
            None,
        ),
    ]
}

// ---- argument helpers ----

type Args = Map<String, Value>;

fn opt_str<'a>(a: &'a Args, key: &str) -> std::result::Result<Option<&'a str>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(format!("{key} must be a string")),
    }
}

fn req_str<'a>(a: &'a Args, key: &str) -> std::result::Result<&'a str, String> {
    opt_str(a, key)?.ok_or_else(|| format!("{key} is required"))
}

fn kind_arg(a: &Args) -> std::result::Result<EntityKind, String> {
    let k = req_str(a, "kind")?;
    EntityKind::parse(k).ok_or_else(|| format!("kind must be one of monks | temples | amulets (got \"{k}\")"))
}

fn opt_int(a: &Args, key: &str, min: i64, max: i64) -> std::result::Result<Option<i64>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => match v.as_i64().or_else(|| v.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64)) {
            Some(n) if (min..=max).contains(&n) => Ok(Some(n)),
            Some(_) => Err(format!("{key} must be between {min} and {max}")),
            None => Err(format!("{key} must be an integer")),
        },
    }
}

fn app_err(e: AppError) -> String {
    match &e.detail {
        Some(d) => format!("{} ({d})", e.message),
        None => e.message,
    }
}

/// JSON object result as pretty text (the former server used `JSON.stringify(value, null, 2)`).
fn json_text<T: Serialize>(v: &T) -> ToolResult {
    serde_json::to_string_pretty(v).map(|s| vec![Content::Text(s)]).map_err(|e| e.to_string())
}

fn text(s: impl Into<String>) -> ToolResult {
    Ok(vec![Content::Text(s.into())])
}

/// Execute a tool. Blocking (file / git / CLI work) — call from `spawn_blocking`. `None` = unknown tool name.
pub fn call(core: &Core, name: &str, args: &Args) -> Option<ToolResult> {
    Some(match name {
        "list_entities" => list_entities(core, args),
        "get_entity" => get_entity(core, args),
        "entity_template" => entity_template_tool(args),
        "save_entity" => save_entity(core, args),
        "list_sources" => list_sources(core, args),
        "read_source" => read_source(core, args),
        "search_sources" => search_sources(core, args),
        "web_research" => web_research(core, args),
        "sync_status" => core.sync_status().map_err(app_err).and_then(|s| json_text(&s)),
        "sync_data" => sync_data(core, args),
        "update_draft" => update_draft(args),
        _ => return None,
    })
}

fn list_entities(core: &Core, a: &Args) -> ToolResult {
    let kind = kind_arg(a)?;
    let items = core.list_entities(kind).map_err(app_err)?;
    json_text(&json!({"kind": kind.as_str(), "count": items.len(), "items": items}))
}

fn get_entity(core: &Core, a: &Args) -> ToolResult {
    let kind = kind_arg(a)?;
    let slug = assert_slug(req_str(a, "slug")?).map_err(app_err)?;
    let rec = core.get_entity(kind, &slug).map_err(app_err)?.ok_or_else(|| format!("{}/{slug} not found", kind.as_str()))?;
    json_text(&json!({"kind": kind.as_str(), "slug": slug, "path": rec.path, "name": rec.name, "status": rec.status, "markdown": rec.raw}))
}

fn entity_template_tool(a: &Args) -> ToolResult {
    let kind = kind_arg(a)?;
    text(entity_template(kind, opt_str(a, "name")?.unwrap_or("")))
}

fn save_entity(core: &Core, a: &Args) -> ToolResult {
    let kind = kind_arg(a)?;
    let markdown = req_str(a, "markdown")?;
    let message = opt_str(a, "message")?.filter(|m| !m.is_empty());
    let data = frontmatter::parse(markdown).map_err(app_err)?.data;
    let slug = match opt_str(a, "slug")?.filter(|s| !s.is_empty()) {
        Some(s) => assert_slug(s).map_err(app_err)?,
        None => core.derive_slug(kind, &data).map_err(app_err)?,
    };
    let rec = core.write_entity(kind, &slug, markdown, Default::default()).map_err(app_err)?;
    let commit = core.auto_commit_if_enabled(&message.map(str::to_string).unwrap_or_else(|| format!("data({}): {}", kind.as_str(), rec.name)));
    json_text(&json!({
        "ok": true, "kind": kind.as_str(), "slug": slug, "path": rec.path, "name": rec.name, "status": rec.status,
        "committed": commit.map(|c| c.ok).unwrap_or(false)
    }))
}

fn list_sources(core: &Core, a: &Args) -> ToolResult {
    let files = core.list_sources(opt_str(a, "subdir")?.unwrap_or("")).map_err(app_err)?;
    let files: Vec<Value> = files.iter().map(|f| json!({"path": f.path, "kind": f.kind, "size": f.size, "mtime": f.mtime})).collect();
    json_text(&json!({"count": files.len(), "files": files}))
}

fn read_source(core: &Core, a: &Args) -> ToolResult {
    let path = req_str(a, "path")?;
    let max = opt_int(a, "maxChars", 100, 200_000)?.unwrap_or(40_000) as usize;
    let src = core.read_source(path, Some(max)).map_err(app_err)?;
    if src.kind == SourceFileKind::Image {
        if let (Some(data), Some(mime)) = (&src.base64, &src.mime_type) {
            return Ok(vec![Content::Text(format!("Image source: {}", src.path)), Content::Image { data: data.clone(), mime_type: mime.clone() }]);
        }
    }
    match &src.text {
        Some(t) => text(format!("# Source: {}\n\n{t}", src.path)),
        None => text(format!("Source {} is a {} file ({} bytes); its content cannot be read as text.", src.path, src.kind.as_str(), src.size)),
    }
}

fn search_sources(core: &Core, a: &Args) -> ToolResult {
    let query = req_str(a, "query")?;
    let limit = opt_int(a, "limit", 1, 50)?.unwrap_or(20) as usize;
    let hits = if query.trim().is_empty() { vec![] } else { core.search_sources(query, limit).map_err(app_err)? };
    json_text(&json!({"query": query, "count": hits.len(), "hits": hits}))
}

fn web_research(core: &Core, a: &Args) -> ToolResult {
    let query = req_str(a, "query")?;
    let images: Vec<String> = match a.get("images") {
        None | Some(Value::Null) => vec![],
        Some(Value::Array(v)) if v.len() <= 8 && v.iter().all(Value::is_string) => v.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
        Some(_) => return Err("images must be an array of at most 8 strings".into()),
    };
    let no_cache = match a.get("noCache") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err("noCache must be a boolean".into()),
    };
    let res = core
        .web_research(query, opt_str(a, "context")?.map(str::to_string), images, opt_str(a, "ocrText")?.map(str::to_string), no_cache)
        .map_err(app_err)?;
    text(format!(
        "# Web research (UNVERIFIED — {}{}{})\n\n{}",
        res.engine.as_deref().unwrap_or("Gemini"),
        res.model.as_deref().map(|m| format!(" {m}")).unwrap_or_default(),
        if res.cached { ", cached" } else { "" },
        res.text
    ))
}

fn sync_data(core: &Core, a: &Args) -> ToolResult {
    let message = opt_str(a, "message")?;
    let r = match opt_str(a, "mode")? {
        Some("commit") => core.commit_data(message),
        Some("pull") => core.pull_data(),
        Some("push") => core.push_data(),
        Some("sync") | None => core.sync_data(message),
        Some(other) => return Err(format!("mode must be one of commit | pull | push | sync (got \"{other}\")")),
    };
    json_text(&r.map_err(app_err)?)
}

fn update_draft(a: &Args) -> ToolResult {
    if req_str(a, "markdown")?.is_empty() {
        return Err("markdown must not be empty".into());
    }
    opt_str(a, "summary")?;
    text(UPDATE_DRAFT_ACK)
}

// ---- resource: phrapedia://{kind}/{slug} ----

pub const RESOURCE_TEMPLATE: &str = "phrapedia://{kind}/{slug}";

fn percent_encode(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{b:02X}"));
        }
    }
    o
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if let Some(v) = b.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// (uri, name) of every saved record.
pub fn list_resources(core: &Core) -> Vec<(String, String)> {
    let mut v = vec![];
    for kind in EntityKind::ALL {
        for e in core.list_entities(kind).unwrap_or_default() {
            v.push((format!("phrapedia://{}/{}", kind.as_str(), percent_encode(&e.slug)), e.name));
        }
    }
    v
}

/// Markdown text of `phrapedia://{kind}/{slug}`.
pub fn read_resource(core: &Core, uri: &str) -> std::result::Result<String, ResourceError> {
    let rest = uri.strip_prefix("phrapedia://").ok_or_else(|| ResourceError::NotFound(uri.to_string()))?;
    let (kind, slug) = rest.split_once('/').ok_or_else(|| ResourceError::NotFound(uri.to_string()))?;
    let kind = assert_kind(&percent_decode(kind)).map_err(|e| ResourceError::Invalid(e.message))?;
    let slug = assert_slug(&percent_decode(slug)).map_err(|e| ResourceError::Invalid(e.message))?;
    match core.get_entity(kind, &slug) {
        Ok(Some(r)) => Ok(r.raw),
        Ok(None) => Err(ResourceError::NotFound(format!("{}/{slug} not found", kind.as_str()))),
        Err(e) => Err(ResourceError::Invalid(app_err(e))),
    }
}

#[derive(Debug, PartialEq)]
pub enum ResourceError {
    NotFound(String),
    Invalid(String),
}

// ---- prompt: research_record ----

pub const PROMPT_NAME: &str = "research_record";

/// Guided workflow text (Japanese). `kind` is monks | temples.
pub fn research_record_prompt(a: &Args) -> std::result::Result<String, String> {
    let kind = req_str(a, "kind")?;
    let label = match kind {
        "monks" => "高僧",
        "temples" => "寺院",
        other => return Err(format!("kind must be one of monks | temples (got \"{other}\")")),
    };
    let name = req_str(a, "name")?;
    Ok([
        format!("「{name}」という{label}のレコードを作成してください。手順:"),
        "1. search_sources / read_source で一次資料（確定情報）を集める。画像資料も read_source で確認する。".to_string(),
        "2. 一次資料で不明な点だけ web_research で調べ、結果は「未確認」として扱う。".to_string(),
        format!("3. entity_template(kind=\"{kind}\") の構成に沿って Markdown を作り、出典セクションに一次資料パスと Web URL を分けて記載する。"),
        "4. save_entity で保存する（既存なら list_entities で slug を確認し更新）。".to_string(),
    ]
    .join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::testutil::core_with_dataset;

    fn args(v: Value) -> Args {
        v.as_object().unwrap().clone()
    }
    fn text_of(r: ToolResult) -> String {
        match r.expect("tool ok").remove(0) {
            Content::Text(t) => t,
            other => panic!("not text: {other:?}"),
        }
    }
    fn run(core: &Core, name: &str, v: Value) -> ToolResult {
        call(core, name, &args(v)).expect("known tool")
    }
    const MONK: &str = "---\nslug: ''\nkind: monks\nname: ソムデット\nname_th: สมเด็จ\nname_en: Somdet To\nstatus: draft\nsources: []\ntags: []\n---\n# ソムデット\n\n## 概要\n本文\n";

    #[test]
    fn names_match_specs_and_the_update_draft_tool_is_documented_as_chat_only() {
        let s = specs();
        assert_eq!(s.iter().map(|t| t.name).collect::<Vec<_>>(), TOOL_NAMES);
        for t in &s {
            assert_eq!(t.schema["type"], "object", "{}", t.name);
            assert!(t.description.len() > 20 && !t.title.is_empty());
        }
        let ud = s.iter().find(|t| t.name == "update_draft").unwrap();
        assert!(ud.description.contains("in-app chat ONLY"));
        assert_eq!(ud.schema["required"], json!(["markdown"]));
        let wr = s.iter().find(|t| t.name == "web_research").unwrap();
        assert!(wr.description.contains("この画像の…"));
    }

    #[test]
    fn save_get_list_roundtrip_in_a_temp_dataset() {
        let (_t, core) = core_with_dataset();
        // empty at first
        let v: Value = serde_json::from_str(&text_of(run(&core, "list_entities", json!({"kind": "monks"})))).unwrap();
        assert_eq!((v["kind"].as_str(), v["count"].as_u64()), (Some("monks"), Some(0)));
        // create (slug derived from name_en)
        let saved: Value = serde_json::from_str(&text_of(run(&core, "save_entity", json!({"kind": "monks", "markdown": MONK})))).unwrap();
        assert_eq!(saved["ok"], true);
        assert_eq!(saved["slug"], "somdet-to");
        assert_eq!(saved["path"], "data/monks/somdet-to.md");
        assert_eq!(saved["committed"], false);
        // list sees it
        let v: Value = serde_json::from_str(&text_of(run(&core, "list_entities", json!({"kind": "monks"})))).unwrap();
        assert_eq!(v["count"], 1);
        assert_eq!(v["items"][0]["slug"], "somdet-to");
        assert_eq!(v["items"][0]["name"], "ソムデット");
        // get returns the full markdown
        let g: Value = serde_json::from_str(&text_of(run(&core, "get_entity", json!({"kind": "monks", "slug": "somdet-to"})))).unwrap();
        assert_eq!(g["name"], "ソムデット");
        assert!(g["markdown"].as_str().unwrap().contains("## 概要\n本文"));
        // update with an explicit slug overwrites
        let again = MONK.replace("本文", "更新後");
        run(&core, "save_entity", json!({"kind": "monks", "slug": "somdet-to", "markdown": again})).unwrap();
        let g: Value = serde_json::from_str(&text_of(run(&core, "get_entity", json!({"kind": "monks", "slug": "somdet-to"})))).unwrap();
        assert!(g["markdown"].as_str().unwrap().contains("更新後"));
        // the chat's `saved` event parser accepts exactly this output
        let parsed = crate::chat::stream::parse_saved_result(&serde_json::to_string_pretty(&saved).unwrap());
        assert_eq!(parsed, Some((EntityKind::Monks, "somdet-to".to_string(), Some("data/monks/somdet-to.md".to_string()))));
    }

    #[test]
    fn errors_are_messages_not_panics() {
        let (_t, core) = core_with_dataset();
        assert_eq!(run(&core, "get_entity", json!({"kind": "monks", "slug": "nope"})).unwrap_err(), "monks/nope not found");
        assert!(run(&core, "get_entity", json!({"kind": "bogus", "slug": "x"})).unwrap_err().contains("kind must be one of"));
        assert!(run(&core, "get_entity", json!({"kind": "monks", "slug": "../x"})).unwrap_err().contains("slug"));
        assert_eq!(run(&core, "list_entities", json!({})).unwrap_err(), "kind is required");
        assert!(run(&core, "search_sources", json!({"query": "a", "limit": 99})).unwrap_err().contains("between 1 and 50"));
        assert!(run(&core, "read_source", json!({"path": "a.md", "maxChars": 5})).unwrap_err().contains("between 100"));
        assert!(run(&core, "read_source", json!({"path": "../etc/passwd"})).is_err());
        assert!(run(&core, "update_draft", json!({"markdown": ""})).is_err());
        assert!(run(&core, "sync_data", json!({"mode": "explode"})).unwrap_err().contains("mode must be"));
        assert!(call(&core, "no_such_tool", &Args::new()).is_none());
    }

    #[test]
    fn sources_list_search_and_read() {
        let (_t, core) = core_with_dataset();
        let dir = core.dataset_root().unwrap().join("sources/notes");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("somdet.md"), "# ソムデット\nวัดระฆังโฆสิตาราม\n").unwrap();
        std::fs::write(dir.join("pic.png"), b"\x89PNG\r\n\x1a\n").unwrap();
        let l: Value = serde_json::from_str(&text_of(run(&core, "list_sources", json!({})))).unwrap();
        assert_eq!(l["count"], 2);
        assert_eq!(l["files"][0].as_object().unwrap().keys().cloned().collect::<Vec<_>>().len(), 4);
        let l: Value = serde_json::from_str(&text_of(run(&core, "list_sources", json!({"subdir": "notes"})))).unwrap();
        assert_eq!(l["count"], 2);
        let s: Value = serde_json::from_str(&text_of(run(&core, "search_sources", json!({"query": "วัดระฆัง"})))).unwrap();
        assert_eq!(s["count"], 1);
        assert_eq!(s["hits"][0]["path"], "notes/somdet.md");
        assert_eq!(s["query"], "วัดระฆัง");
        let blank: Value = serde_json::from_str(&text_of(run(&core, "search_sources", json!({"query": "  "})))).unwrap();
        assert_eq!(blank["count"], 0);
        // text read
        assert_eq!(text_of(run(&core, "read_source", json!({"path": "notes/somdet.md"}))), "# Source: notes/somdet.md\n\n# ソムデット\nวัดระฆังโฆสิตาราม\n");
        // image read → text + image content
        let img = run(&core, "read_source", json!({"path": "notes/pic.png"})).unwrap();
        assert_eq!(img.len(), 2);
        assert_eq!(img[0], Content::Text("Image source: notes/pic.png".into()));
        assert!(matches!(&img[1], Content::Image { mime_type, data } if mime_type == "image/png" && !data.is_empty()));
    }

    #[test]
    fn template_status_and_update_draft_ack() {
        let (_t, core) = core_with_dataset();
        let t = text_of(run(&core, "entity_template", json!({"kind": "temples", "name": "ワット・X"})));
        assert!(t.contains("kind: temples") && t.contains("# ワット・X") && t.contains("## 歴史"));
        let st: Value = serde_json::from_str(&text_of(run(&core, "sync_status", json!({})))).unwrap();
        assert_eq!(st["isRepo"], false);
        assert_eq!(text_of(run(&core, "update_draft", json!({"markdown": "# a", "summary": "x"}))), UPDATE_DRAFT_ACK);
    }

    #[test]
    fn resources_list_and_read() {
        let (_t, core) = core_with_dataset();
        run(&core, "save_entity", json!({"kind": "monks", "markdown": MONK})).unwrap();
        assert_eq!(list_resources(&core), vec![("phrapedia://monks/somdet-to".to_string(), "ソムデット".to_string())]);
        assert!(read_resource(&core, "phrapedia://monks/somdet-to").unwrap().contains("## 概要"));
        assert!(matches!(read_resource(&core, "phrapedia://monks/none"), Err(ResourceError::NotFound(_))));
        assert!(matches!(read_resource(&core, "phrapedia://bogus/x"), Err(ResourceError::Invalid(_))));
        assert!(matches!(read_resource(&core, "http://x"), Err(ResourceError::NotFound(_))));
        assert_eq!(percent_decode(&percent_encode("a b/あ")), "a b/あ");
    }

    #[test]
    fn prompt_text_per_kind() {
        let p = research_record_prompt(&args(json!({"kind": "monks", "name": "ソムデット"}))).unwrap();
        assert!(p.starts_with("「ソムデット」という高僧のレコードを作成してください。手順:\n1. "));
        assert!(p.contains("entity_template(kind=\"monks\")"));
        assert!(research_record_prompt(&args(json!({"kind": "temples", "name": "X"}))).unwrap().contains("という寺院の"));
        assert!(research_record_prompt(&args(json!({"kind": "amulets", "name": "X"}))).is_err());
        assert!(research_record_prompt(&args(json!({"kind": "monks"}))).is_err());
    }
}
