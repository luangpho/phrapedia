//! Tauri command layer: thin wrappers over `crate::core`, one per REST endpoint of the old Nitro server.
//! The contract (names, arguments, return values, errors) is documented in docs/NATIVE_API.md.
//! Every command runs on the blocking pool; errors are `AppError` (`{ code, message, detail? }`).

use crate::core::entities::{assert_kind, assert_slug, entity_template as make_template, WriteOptions};
use crate::core::error::{AppError, Result};
use crate::core::frontmatter;
use crate::core::types::*;
use crate::core::{AppInfo, Core, PathSettingsView, SavePathsOutcome, SettingsView};
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::ipc::Response;
use tauri::State;

type CoreState<'a> = State<'a, Arc<Core>>;

async fn blocking<T: Send + 'static>(core: CoreState<'_>, f: impl FnOnce(&Core) -> Result<T> + Send + 'static) -> Result<T> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || f(&core))
        .await
        .map_err(|e| AppError::internal("バックグラウンド処理が異常終了しました").with_detail(e.to_string()))?
}

// ---- app / path settings ----

#[tauri::command]
pub async fn app_info(core: CoreState<'_>) -> Result<AppInfo> {
    blocking(core, |c| Ok(c.app_info())).await
}

#[tauri::command]
pub async fn get_path_settings(core: CoreState<'_>) -> Result<PathSettingsView> {
    blocking(core, |c| Ok(c.path_settings_view())).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn save_path_settings(core: CoreState<'_>, dataset_path: String, export_path: String) -> Result<SavePathsOutcome> {
    blocking(core, move |c| c.save_path_settings(&dataset_path, &export_path)).await
}

// ---- AppSettings ----

#[tauri::command]
pub async fn get_settings(core: CoreState<'_>) -> Result<SettingsView> {
    blocking(core, |c| c.settings_view()).await
}

#[tauri::command]
pub async fn update_settings(core: CoreState<'_>, patch: Value) -> Result<crate::core::app_settings::AppSettings> {
    blocking(core, move |c| c.update_settings(&patch)).await
}

#[tauri::command]
pub async fn reindex_sources(core: CoreState<'_>) -> Result<ReindexResult> {
    blocking(core, |c| c.reindex_sources()).await
}

// ---- entities ----

#[tauri::command]
pub async fn list_entities(core: CoreState<'_>, kind: String) -> Result<Vec<EntitySummary>> {
    let kind = assert_kind(&kind)?;
    blocking(core, move |c| c.list_entities(kind)).await
}

#[tauri::command]
pub async fn entity_template(kind: String, name: Option<String>) -> Result<String> {
    Ok(make_template(assert_kind(&kind)?, name.as_deref().unwrap_or("")))
}

#[tauri::command]
pub async fn get_entity(core: CoreState<'_>, kind: String, slug: String) -> Result<EntityRecord> {
    let kind = assert_kind(&kind)?;
    let slug = assert_slug(&slug)?;
    blocking(core, move |c| c.get_entity(kind, &slug)?.ok_or_else(|| AppError::not_found(format!("{}/{slug} が見つかりません", kind.as_str())))).await
}

#[tauri::command]
pub async fn create_entity(core: CoreState<'_>, kind: String, raw: String, message: Option<String>) -> Result<SaveEntityResult> {
    let kind = assert_kind(&kind)?;
    if raw.is_empty() {
        return Err(AppError::invalid("raw（Markdown）が必要です"));
    }
    blocking(core, move |c| {
        let data = frontmatter::parse(&raw)?.data;
        let slug = c.derive_slug(kind, &data)?;
        let record = c.write_entity(kind, &slug, &raw, WriteOptions { must_not_exist: true, ..Default::default() })?;
        let commit = c.auto_commit_if_enabled(&message.unwrap_or_else(|| format!("data({}): add {}", kind.as_str(), record.name)));
        Ok(SaveEntityResult { record, commit })
    })
    .await
}

#[tauri::command]
pub async fn update_entity(core: CoreState<'_>, kind: String, slug: String, raw: String, message: Option<String>) -> Result<SaveEntityResult> {
    let kind = assert_kind(&kind)?;
    let slug = assert_slug(&slug)?;
    if raw.is_empty() {
        return Err(AppError::invalid("raw（Markdown）が必要です"));
    }
    blocking(core, move |c| {
        let record = c.write_entity(kind, &slug, &raw, WriteOptions { must_exist: true, ..Default::default() })?;
        let commit = c.auto_commit_if_enabled(&message.unwrap_or_else(|| format!("data({}): update {}", kind.as_str(), record.name)));
        Ok(SaveEntityResult { record, commit })
    })
    .await
}

#[tauri::command]
pub async fn delete_entity(core: CoreState<'_>, kind: String, slug: String) -> Result<Value> {
    let kind = assert_kind(&kind)?;
    let slug = assert_slug(&slug)?;
    blocking(core, move |c| {
        c.delete_entity(kind, &slug)?;
        let commit = c.auto_commit_if_enabled(&format!("data({}): remove {slug}", kind.as_str()));
        Ok(json!({ "ok": true, "commit": commit }))
    })
    .await
}

// ---- sources ----

#[tauri::command]
pub async fn list_sources(core: CoreState<'_>, dir: Option<String>) -> Result<Vec<SourceFile>> {
    blocking(core, move |c| c.list_sources(dir.as_deref().unwrap_or(""))).await
}

#[tauri::command]
pub async fn search_sources(core: CoreState<'_>, q: String, limit: Option<u32>) -> Result<Vec<SourceSearchHit>> {
    if q.trim().is_empty() {
        return Ok(vec![]);
    }
    let limit = limit.unwrap_or(20).clamp(1, 50) as usize;
    blocking(core, move |c| c.search_sources(&q, limit)).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn read_source(core: CoreState<'_>, path: String, max_chars: Option<usize>) -> Result<SourceContent> {
    blocking(core, move |c| c.read_source(&path, max_chars)).await
}

/// Raw bytes (ArrayBuffer in JS) for `<img>` / `<iframe>` via a Blob URL.
#[tauri::command]
pub async fn read_source_bytes(core: CoreState<'_>, path: String) -> Result<Response> {
    let (bytes, _) = blocking(core, move |c| c.read_source_bytes(&path)).await?;
    Ok(Response::new(bytes))
}

// ---- web research ----

#[tauri::command(rename_all = "camelCase")]
pub async fn web_research(core: CoreState<'_>, query: String, context: Option<String>, images: Option<Vec<String>>, ocr_text: Option<String>, no_cache: Option<bool>) -> Result<ResearchResult> {
    blocking(core, move |c| c.web_research(&query, context, images.unwrap_or_default(), ocr_text, no_cache.unwrap_or(false))).await
}

// ---- git sync ----

#[tauri::command]
pub async fn sync_status(core: CoreState<'_>) -> Result<SyncStatus> {
    blocking(core, |c| c.sync_status()).await
}

#[tauri::command]
pub async fn commit_data(core: CoreState<'_>, message: Option<String>) -> Result<SyncResult> {
    blocking(core, move |c| c.commit_data(message.as_deref())).await
}

#[tauri::command]
pub async fn pull_data(core: CoreState<'_>) -> Result<SyncResult> {
    blocking(core, |c| c.pull_data()).await
}

#[tauri::command]
pub async fn push_data(core: CoreState<'_>) -> Result<SyncResult> {
    blocking(core, |c| c.push_data()).await
}

#[tauri::command]
pub async fn sync_data(core: CoreState<'_>, message: Option<String>) -> Result<SyncResult> {
    blocking(core, move |c| c.sync_data(message.as_deref())).await
}

// ---- chat history ----

#[tauri::command]
pub async fn list_chat_sessions(core: CoreState<'_>, kind: String, slug: Option<String>) -> Result<Vec<ChatSession>> {
    let kind = assert_kind(&kind)?;
    let slug = slug.filter(|s| !s.is_empty()).map(|s| assert_slug(&s)).transpose()?;
    blocking(core, move |c| c.list_chat_sessions(kind, slug.as_deref())).await
}

fn session_not_found(id: &str) -> AppError {
    AppError::not_found(format!("チャットセッションが見つかりません: {id}"))
}

#[tauri::command]
pub async fn get_chat_session(core: CoreState<'_>, id: String) -> Result<ChatSessionWithMessages> {
    blocking(core, move |c| {
        let session = c.get_chat_session(&id)?.ok_or_else(|| session_not_found(&id))?;
        let messages = c.list_chat_messages(&id)?;
        Ok(ChatSessionWithMessages { session, messages })
    })
    .await
}

#[tauri::command]
pub async fn delete_chat_session(core: CoreState<'_>, id: String) -> Result<Value> {
    blocking(core, move |c| if c.delete_chat_session(&id)? { Ok(json!({ "ok": true })) } else { Err(session_not_found(&id)) }).await
}

/// `patch`: `{ slug?: string | null, title?: string }` — an explicit `null` slug turns the session back into a draft.
#[tauri::command]
pub async fn patch_chat_session(core: CoreState<'_>, id: String, patch: Value) -> Result<ChatSession> {
    let slug: Option<Option<String>> = match patch.get("slug") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(s)) if s.is_empty() => Some(None),
        Some(Value::String(s)) => Some(Some(assert_slug(s)?)),
        Some(_) => return Err(AppError::invalid("slug は文字列か null で指定してください")),
    };
    let title = patch.get("title").and_then(Value::as_str).map(str::to_string);
    blocking(core, move |c| {
        c.get_chat_session(&id)?.ok_or_else(|| session_not_found(&id))?;
        c.touch_chat_session(&id, slug.as_ref().map(|s| s.as_deref()), title.as_deref())?;
        c.get_chat_session(&id)?.ok_or_else(|| session_not_found(&id))
    })
    .await
}

#[tauri::command]
pub async fn delete_chat_messages(core: CoreState<'_>, id: String, ids: Vec<String>) -> Result<Value> {
    let ids: Vec<String> = ids.into_iter().take(100).collect();
    blocking(core, move |c| {
        if ids.is_empty() {
            return Ok(json!({ "ok": true, "deleted": 0 }));
        }
        c.get_chat_session(&id)?.ok_or_else(|| session_not_found(&id))?;
        let deleted = c.delete_chat_messages(&id, &ids)?;
        // The CLI session still remembers the removed turns: start a fresh one (history is re-sent as a summary).
        if deleted > 0 {
            c.set_claude_session_id(&id, None)?;
        }
        Ok(json!({ "ok": true, "deleted": deleted }))
    })
    .await
}
