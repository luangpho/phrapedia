//! `chat_send` (Channel streaming) / `chat_abort`. Contract: docs/NATIVE_API.md §3.

use crate::chat::claude::McpLaunch;
use crate::chat::stream::ChatStreamEvent;
use crate::chat::turn::{prepare_chat_turn, run_chat_turn, validate_request, ChatRequest};
use crate::core::error::{AppError, Result};
use crate::core::Core;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::State;
use tokio::sync::watch;

/// In-flight turns by `turnId`; `chat_abort` flips the matching sender.
#[derive(Default)]
pub struct ChatTurns(Mutex<HashMap<String, watch::Sender<bool>>>);

impl ChatTurns {
    fn register(&self, id: &str) -> Option<(watch::Sender<bool>, watch::Receiver<bool>)> {
        let mut m = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if m.contains_key(id) {
            return None;
        }
        let (tx, rx) = watch::channel(false);
        m.insert(id.to_string(), tx.clone());
        Some((tx, rx))
    }
    fn remove(&self, id: &str) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).remove(id);
    }
    /// True when a turn with that id was running.
    pub fn abort(&self, id: &str) -> bool {
        match self.0.lock().unwrap_or_else(|p| p.into_inner()).get(id) {
            Some(tx) => {
                let _ = tx.send(true);
                true
            }
            None => false,
        }
    }
}

struct Unregister<'a>(&'a ChatTurns, &'a str);
impl Drop for Unregister<'_> {
    fn drop(&mut self) {
        self.0.remove(self.1);
    }
}

fn mcp_launch(core: &Core) -> Result<McpLaunch> {
    let exe = std::env::current_exe().map_err(|e| AppError::io("実行ファイルのパスを取得できません", e))?;
    Ok(McpLaunch { exe, config_dir: core.dirs.config_dir.clone(), data_dir: core.dirs.data_dir.clone() })
}

/// Resolves after the turn finished (`done` / `error` already sent). Only invalid input rejects.
#[tauri::command(rename_all = "camelCase")]
pub async fn chat_send(core: State<'_, Arc<Core>>, turns: State<'_, ChatTurns>, request: ChatRequest, on_event: Channel<ChatStreamEvent>) -> Result<()> {
    let (message, slug) = validate_request(&request)?;
    let core = core.inner().clone();
    let (tx, rx) = turns.register(&request.turn_id).ok_or_else(|| AppError::invalid("同じ turnId のチャットが進行中です"))?;
    let turn_id = request.turn_id.clone();
    let _guard = Unregister(&turns, &turn_id);

    // A closed WebView cannot receive anything any more: stop the CLI instead of letting it run to the timeout.
    let send_tx = tx.clone();
    let mut emit = move |ev: ChatStreamEvent| {
        if on_event.send(ev).is_err() {
            let _ = send_tx.send(true);
        }
    };

    let prepared = {
        let (core, request, message, slug) = (core.clone(), request.clone(), message.clone(), slug.clone());
        tauri::async_runtime::spawn_blocking(move || prepare_chat_turn(&core, &request, &message, slug.as_deref()))
            .await
            .map_err(|e| AppError::internal("バックグラウンド処理が異常終了しました").with_detail(e.to_string()))?
    };
    let prepared = match prepared {
        Ok(p) => p,
        Err(e) => {
            // Same as the old SSE: not-found CLI etc. surface as an `error` event, not as a rejection.
            emit(ChatStreamEvent::Error { message: e.message });
            return Ok(());
        }
    };
    emit(ChatStreamEvent::Session {
        session_id: prepared.session.id.clone(),
        user_message_id: prepared.user_message.id.clone(),
        assistant_message_id: prepared.assistant_id.clone(),
    });
    let mut request = request;
    request.message = message;
    match run_chat_turn(&core, &request, prepared, &mcp_launch(&core)?, rx, &mut emit).await {
        // On provider errors the `error` event was already sent; still deliver the stored message.
        Ok(outcome) => emit(ChatStreamEvent::Done { message: outcome.assistant_message }),
        Err(e) => emit(ChatStreamEvent::Error { message: e.message }),
    }
    Ok(())
}

/// Stops an in-flight turn (SIGTERM to the CLI's process group, SIGKILL after 5 s). No-op for an unknown id.
#[tauri::command(rename_all = "camelCase")]
pub async fn chat_abort(turns: State<'_, ChatTurns>, turn_id: String) -> Result<()> {
    turns.abort(&turn_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abort_flips_only_the_registered_turn_and_unregister_forgets_it() {
        let turns = ChatTurns::default();
        let (_tx, rx) = turns.register("a").unwrap();
        assert!(turns.register("a").is_none(), "duplicate id rejected");
        assert!(!turns.abort("zzz"));
        assert!(!*rx.borrow());
        assert!(turns.abort("a"));
        assert!(*rx.borrow());
        {
            let _g = Unregister(&turns, "a");
        }
        assert!(!turns.abort("a"));
        assert!(turns.register("a").is_some());
    }
}
