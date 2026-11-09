//! Wire types shared with the WebView (mirror shared/types/**). Field names are camelCase on the wire.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityKind {
    Monks,
    Temples,
    Amulets,
}

impl EntityKind {
    pub const ALL: [EntityKind; 3] = [EntityKind::Monks, EntityKind::Temples, EntityKind::Amulets];

    pub fn as_str(self) -> &'static str {
        match self {
            EntityKind::Monks => "monks",
            EntityKind::Temples => "temples",
            EntityKind::Amulets => "amulets",
        }
    }

    pub fn parse(s: &str) -> Option<EntityKind> {
        EntityKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityStatus {
    Draft,
    Reviewed,
    Verified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EntitySummary {
    pub kind: EntityKind,
    pub slug: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_th: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_en: Option<String>,
    pub status: EntityStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EntityRecord {
    pub kind: EntityKind,
    pub slug: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_th: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_en: Option<String>,
    pub status: EntityStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Parsed frontmatter (YAML keys as written: snake_case)
    pub frontmatter: Map<String, Value>,
    pub body: String,
    pub raw: String,
    /// POSIX path relative to the dataset root (e.g. data/monks/x.md)
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveEntityResult {
    pub record: EntityRecord,
    pub commit: Option<SyncResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceFileKind {
    Markdown,
    Text,
    Image,
    Pdf,
    Other,
}

impl SourceFileKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceFileKind::Markdown => "markdown",
            SourceFileKind::Text => "text",
            SourceFileKind::Image => "image",
            SourceFileKind::Pdf => "pdf",
            SourceFileKind::Other => "other",
        }
    }
    pub fn parse(s: &str) -> SourceFileKind {
        match s {
            "markdown" => SourceFileKind::Markdown,
            "text" => SourceFileKind::Text,
            "image" => SourceFileKind::Image,
            "pdf" => SourceFileKind::Pdf,
            _ => SourceFileKind::Other,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceFile {
    pub path: String,
    pub name: String,
    /// With the leading dot (".md") like node's extname; empty when none
    pub ext: String,
    pub kind: SourceFileKind,
    pub size: u64,
    pub mtime: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceSearchHit {
    pub path: String,
    pub kind: SourceFileKind,
    pub snippet: String,
    pub score: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceContent {
    pub path: String,
    pub kind: SourceFileKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReindexResult {
    pub indexed: usize,
    pub removed: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncFileChange {
    pub path: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LastCommit {
    pub hash: String,
    pub message: String,
    pub date: String,
    pub author: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub is_repo: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    pub ahead: i64,
    pub behind: i64,
    pub changes: Vec<SyncFileChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_commit: Option<LastCommit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncAction {
    Commit,
    Push,
    Pull,
    Sync,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub ok: bool,
    pub action: SyncAction,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<SyncStatus>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchResult {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub cached: bool,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}

// ---- chat ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    User,
    Assistant,
    System,
}

impl ChatRole {
    pub fn as_str(self) -> &'static str {
        match self {
            ChatRole::User => "user",
            ChatRole::Assistant => "assistant",
            ChatRole::System => "system",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolState {
    Running,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum ChatPart {
    Text {
        text: String,
    },
    Tool {
        tool_call_id: String,
        name: String,
        #[serde(default)]
        input: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
        state: ToolState,
    },
    Draft {
        markdown: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub role: ChatRole,
    pub parts: Vec<ChatPart>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatSession {
    pub id: String,
    pub kind: EntityKind,
    pub slug: Option<String>,
    pub title: String,
    pub claude_session_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatSessionWithMessages {
    pub session: ChatSession,
    pub messages: Vec<ChatMessage>,
}
