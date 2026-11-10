//! SQLite store (`<app data dir>/state/phrapedia.sqlite`): chat sessions / messages, primary-source index (FTS5),
//! web-research cache. Replaces PGlite. Shared between the GUI process and `phrapedia mcp` (WAL + busy_timeout).

use super::error::{AppError, Result};
use super::types::{ChatMessage, ChatPart, ChatRole, ChatSession, EntityKind};
use super::Core;
use rusqlite::{params, Connection, OptionalExtension};
use std::fs;

const MIGRATIONS: &[&str] = &[
    // 1
    "CREATE TABLE chat_sessions (
        id TEXT PRIMARY KEY,
        kind TEXT NOT NULL,
        slug TEXT,
        title TEXT NOT NULL DEFAULT '',
        claude_session_id TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE INDEX chat_sessions_kind_slug ON chat_sessions (kind, slug);
    CREATE TABLE chat_messages (
        id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
        role TEXT NOT NULL,
        parts TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE INDEX chat_messages_session ON chat_messages (session_id, created_at);
    CREATE TABLE source_index (
        path TEXT PRIMARY KEY,
        kind TEXT NOT NULL,
        size INTEGER NOT NULL,
        mtime TEXT NOT NULL,
        content TEXT,
        indexed_at TEXT NOT NULL
    );
    CREATE VIRTUAL TABLE source_fts USING fts5(content, content='source_index', content_rowid='rowid', tokenize='trigram');
    CREATE TABLE research_cache (
        query_hash TEXT PRIMARY KEY,
        query TEXT NOT NULL,
        result TEXT NOT NULL,
        model TEXT,
        created_at TEXT NOT NULL
    );",
];

/// Open (and migrate) the store. A file that is not a SQLite database / is corrupt is set aside as
/// `<name>.corrupt-<unix time>` and a fresh store is created: the data in it (chat history, search index, research cache)
/// is rebuildable or disposable, and the records themselves are Markdown files.
pub fn open(path: &std::path::Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::io("状態保存フォルダを作れません", e))?;
    }
    match open_raw(path) {
        Err(e) if is_corrupt(&e) => {
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            for suffix in ["", "-wal", "-shm"] {
                let mut from = path.as_os_str().to_owned();
                from.push(suffix);
                let mut to = path.as_os_str().to_owned();
                to.push(format!(".corrupt-{stamp}{suffix}"));
                let _ = fs::rename(&from, &to);
            }
            eprintln!("phrapedia: {} was not a usable database ({e}); moved aside and recreated", path.display());
            Ok(open_raw(path)?)
        }
        other => Ok(other?),
    }
}

fn is_corrupt(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(f, _)
            if matches!(f.code, rusqlite::ErrorCode::NotADatabase | rusqlite::ErrorCode::DatabaseCorrupt)
    )
}

fn open_raw(path: &std::path::Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA synchronous=NORMAL;")?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let current: usize = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        // BEGIN IMMEDIATE so two processes starting at once do not both run the same migration
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let applied: usize = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if applied > i {
            conn.execute_batch("COMMIT")?;
            continue;
        }
        let r = conn.execute_batch(sql).and_then(|_| conn.execute_batch(&format!("PRAGMA user_version = {}", i + 1)));
        match r {
            Ok(()) => conn.execute_batch("COMMIT")?,
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(e);
            }
        }
    }
    Ok(())
}

fn now() -> String {
    super::entities::now_iso()
}

fn row_session(r: &rusqlite::Row<'_>) -> rusqlite::Result<ChatSession> {
    let kind: String = r.get(1)?;
    Ok(ChatSession {
        id: r.get(0)?,
        kind: EntityKind::parse(&kind).unwrap_or(EntityKind::Monks),
        slug: r.get(2)?,
        title: r.get(3)?,
        claude_session_id: r.get(4)?,
        created_at: r.get(5)?,
        updated_at: r.get(6)?,
    })
}

const SESSION_COLS: &str = "id, kind, slug, title, claude_session_id, created_at, updated_at";

impl Core {
    /// Run `f` with the (lazily opened) SQLite connection.
    pub fn with_db<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let mut guard = self.db.lock().unwrap_or_else(|p| p.into_inner());
        if guard.is_none() {
            *guard = Some(open(&self.dirs.state_dir().join("phrapedia.sqlite"))?);
        }
        f(guard.as_ref().expect("opened above"))
    }

    pub fn create_chat_session(&self, kind: EntityKind, slug: Option<&str>, title: &str) -> Result<ChatSession> {
        let id = uuid::Uuid::new_v4().to_string();
        let t = now();
        let title: String = title.chars().take(200).collect();
        self.with_db(|c| {
            c.execute(
                "INSERT INTO chat_sessions (id, kind, slug, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![id, kind.as_str(), slug, title, t],
            )?;
            Ok(c.query_row(&format!("SELECT {SESSION_COLS} FROM chat_sessions WHERE id = ?1"), [&id], row_session)?)
        })
    }

    pub fn get_chat_session(&self, id: &str) -> Result<Option<ChatSession>> {
        self.with_db(|c| Ok(c.query_row(&format!("SELECT {SESSION_COLS} FROM chat_sessions WHERE id = ?1"), [id], row_session).optional()?))
    }

    /// Sessions of a record, newest first. `slug == None` lists draft (unsaved) sessions of the kind.
    pub fn list_chat_sessions(&self, kind: EntityKind, slug: Option<&str>) -> Result<Vec<ChatSession>> {
        self.with_db(|c| {
            let rows = match slug {
                None => {
                    let mut st = c.prepare(&format!("SELECT {SESSION_COLS} FROM chat_sessions WHERE kind = ?1 AND slug IS NULL ORDER BY updated_at DESC LIMIT 100"))?;
                    let r = st.query_map([kind.as_str()], row_session)?.collect::<rusqlite::Result<Vec<_>>>()?;
                    r
                }
                Some(s) => {
                    let mut st = c.prepare(&format!("SELECT {SESSION_COLS} FROM chat_sessions WHERE kind = ?1 AND slug = ?2 ORDER BY updated_at DESC LIMIT 100"))?;
                    let r = st.query_map(params![kind.as_str(), s], row_session)?.collect::<rusqlite::Result<Vec<_>>>()?;
                    r
                }
            };
            Ok(rows)
        })
    }

    /// Insert a message, or overwrite the parts of an existing one with the same id.
    pub fn append_chat_message(&self, session_id: &str, m: &ChatMessage) -> Result<()> {
        let parts = serde_json::to_string(&m.parts)?;
        self.with_db(|c| {
            c.execute(
                "INSERT INTO chat_messages (id, session_id, role, parts, created_at) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (id) DO UPDATE SET parts = excluded.parts",
                params![m.id, session_id, m.role.as_str(), parts, m.created_at],
            )?;
            Ok(())
        })
    }

    pub fn list_chat_messages(&self, session_id: &str) -> Result<Vec<ChatMessage>> {
        self.with_db(|c| {
            let mut st = c.prepare("SELECT id, role, parts, created_at FROM chat_messages WHERE session_id = ?1 ORDER BY created_at ASC, id ASC")?;
            let rows = st
                .query_map([session_id], |r| {
                    let role: String = r.get(1)?;
                    let parts: String = r.get(2)?;
                    Ok(ChatMessage {
                        id: r.get(0)?,
                        role: match role.as_str() {
                            "assistant" => ChatRole::Assistant,
                            "system" => ChatRole::System,
                            _ => ChatRole::User,
                        },
                        parts: serde_json::from_str::<Vec<ChatPart>>(&parts).unwrap_or_default(),
                        created_at: r.get(3)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    pub fn delete_chat_session(&self, id: &str) -> Result<bool> {
        self.with_db(|c| Ok(c.execute("DELETE FROM chat_sessions WHERE id = ?1", [id])? > 0))
    }

    /// Delete specific messages of a session; returns how many rows were removed.
    pub fn delete_chat_messages(&self, session_id: &str, ids: &[String]) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        self.with_db(|c| {
            let mut n = 0;
            for id in ids {
                n += c.execute("DELETE FROM chat_messages WHERE session_id = ?1 AND id = ?2", params![session_id, id])?;
            }
            Ok(n)
        })
    }

    /// Bump updated_at; optionally re-bind to a slug (`Some(None)` = back to draft) and / or rename.
    pub fn touch_chat_session(&self, id: &str, slug: Option<Option<&str>>, title: Option<&str>) -> Result<()> {
        self.with_db(|c| {
            c.execute("UPDATE chat_sessions SET updated_at = ?2 WHERE id = ?1", params![id, now()])?;
            if let Some(s) = slug {
                c.execute("UPDATE chat_sessions SET slug = ?2 WHERE id = ?1", params![id, s])?;
            }
            if let Some(t) = title {
                let t: String = t.chars().take(200).collect();
                c.execute("UPDATE chat_sessions SET title = ?2 WHERE id = ?1", params![id, t])?;
            }
            Ok(())
        })
    }

    /// Remember (or clear, with None) the Claude Code session that holds this conversation's context.
    pub fn set_claude_session_id(&self, id: &str, claude_session_id: Option<&str>) -> Result<()> {
        self.with_db(|c| {
            c.execute("UPDATE chat_sessions SET claude_session_id = ?2 WHERE id = ?1", params![id, claude_session_id])?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_corrupt_database_file_is_set_aside_and_recreated() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("state").join("phrapedia.sqlite");
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, vec![0x5au8; 4096]).unwrap();
        let conn = open(&p).unwrap();
        let n: i64 = conn.query_row("SELECT count(*) FROM chat_sessions", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
        let moved: Vec<_> = fs::read_dir(p.parent().unwrap()).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().contains(".corrupt-")).collect();
        assert_eq!(moved.len(), 1, "the broken file is kept for inspection");
    }

    use super::super::testutil::core_with_dataset;
    use super::*;
    use serde_json::json;

    fn msg(id: &str, role: ChatRole, text: &str, at: &str) -> ChatMessage {
        ChatMessage { id: id.into(), role, parts: vec![ChatPart::Text { text: text.into() }], created_at: at.into() }
    }

    #[test]
    fn session_lifecycle() {
        let (_t, core) = core_with_dataset();
        let s = core.create_chat_session(EntityKind::Monks, None, "下書き").unwrap();
        assert!(s.slug.is_none() && s.claude_session_id.is_none());
        assert_eq!(core.list_chat_sessions(EntityKind::Monks, None).unwrap().len(), 1);
        assert!(core.list_chat_sessions(EntityKind::Monks, Some("x")).unwrap().is_empty());
        assert!(core.list_chat_sessions(EntityKind::Temples, None).unwrap().is_empty());

        core.set_claude_session_id(&s.id, Some("cs-1")).unwrap();
        core.touch_chat_session(&s.id, Some(Some("somdet-to")), Some("改名")).unwrap();
        let got = core.get_chat_session(&s.id).unwrap().unwrap();
        assert_eq!((got.slug.as_deref(), got.title.as_str(), got.claude_session_id.as_deref()), (Some("somdet-to"), "改名", Some("cs-1")));
        assert_eq!(core.list_chat_sessions(EntityKind::Monks, Some("somdet-to")).unwrap().len(), 1);
        assert!(core.list_chat_sessions(EntityKind::Monks, None).unwrap().is_empty());

        core.touch_chat_session(&s.id, Some(None), None).unwrap();
        assert!(core.get_chat_session(&s.id).unwrap().unwrap().slug.is_none());

        assert!(core.delete_chat_session(&s.id).unwrap());
        assert!(!core.delete_chat_session(&s.id).unwrap());
        assert!(core.get_chat_session(&s.id).unwrap().is_none());
    }

    #[test]
    fn messages_order_overwrite_and_cascade() {
        let (_t, core) = core_with_dataset();
        let s = core.create_chat_session(EntityKind::Temples, Some("wat-a"), "t").unwrap();
        core.append_chat_message(&s.id, &msg("b", ChatRole::Assistant, "返答", "2026-11-05T00:00:02.000Z")).unwrap();
        core.append_chat_message(&s.id, &msg("a", ChatRole::User, "質問", "2026-11-05T00:00:01.000Z")).unwrap();
        let tool = ChatMessage {
            id: "c".into(),
            role: ChatRole::Assistant,
            parts: vec![
                ChatPart::Tool { tool_call_id: "t1".into(), name: "get_entity".into(), input: json!({ "kind": "monks" }), output: Some(json!("ok")), is_error: Some(false), state: super::super::types::ToolState::Done },
                ChatPart::Draft { markdown: "# x".into(), summary: None },
            ],
            created_at: "2026-11-05T00:00:03.000Z".into(),
        };
        core.append_chat_message(&s.id, &tool).unwrap();
        let ids: Vec<_> = core.list_chat_messages(&s.id).unwrap().into_iter().map(|m| m.id).collect();
        assert_eq!(ids, ["a", "b", "c"]);
        // same id: parts are replaced, no duplicate row
        core.append_chat_message(&s.id, &msg("b", ChatRole::Assistant, "更新後", "2026-11-05T00:00:02.000Z")).unwrap();
        let all = core.list_chat_messages(&s.id).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[1].parts, vec![ChatPart::Text { text: "更新後".into() }]);
        assert_eq!(all[2], tool, "tool / draft parts survive the JSON round trip");

        assert_eq!(core.delete_chat_messages(&s.id, &["a".into(), "zzz".into()]).unwrap(), 1);
        assert_eq!(core.delete_chat_messages(&s.id, &[]).unwrap(), 0);
        core.delete_chat_session(&s.id).unwrap();
        assert!(core.list_chat_messages(&s.id).unwrap().is_empty(), "messages cascade with the session");
    }

    #[test]
    fn wire_format_of_parts_matches_ts() {
        let tool = ChatPart::Tool { tool_call_id: "t".into(), name: "n".into(), input: json!(1), output: None, is_error: None, state: super::super::types::ToolState::Running };
        assert_eq!(serde_json::to_value(&tool).unwrap(), json!({ "type": "tool", "toolCallId": "t", "name": "n", "input": 1, "state": "running" }));
        assert_eq!(serde_json::to_value(ChatPart::Draft { markdown: "m".into(), summary: Some("s".into()) }).unwrap(), json!({ "type": "draft", "markdown": "m", "summary": "s" }));
    }

    #[test]
    fn migration_is_idempotent_and_reopen_keeps_data() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("s/x.sqlite");
        {
            let c = open(&p).unwrap();
            c.execute("INSERT INTO research_cache VALUES ('h','q','r',NULL,'t')", []).unwrap();
        }
        let c = open(&p).unwrap();
        let n: i64 = c.query_row("SELECT count(*) FROM research_cache", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, MIGRATIONS.len() as i64);
    }

    #[test]
    fn state_lives_under_app_data_dir() {
        let (_t, core) = core_with_dataset();
        core.create_chat_session(EntityKind::Monks, None, "x").unwrap();
        assert!(core.dirs.data_dir.join("state/phrapedia.sqlite").is_file());
    }
}
