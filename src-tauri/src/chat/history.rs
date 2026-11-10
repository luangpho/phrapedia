//! Plain-text digest of the stored conversation (port of history.ts). Used when the Claude Code CLI session cannot be
//! resumed (other machine, session file gone): the digest is put in front of the prompt so the new session keeps the context.

use crate::core::types::{ChatMessage, ChatPart, ChatRole};

const PART_LIMIT: usize = 1_500;
const TOTAL_LIMIT: usize = 24_000;

fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() > limit {
        format!("{}…(省略)", text.chars().take(limit).collect::<String>())
    } else {
        text.to_string()
    }
}

fn part_text(part: &ChatPart) -> String {
    match part {
        ChatPart::Text { text } => text.trim().to_string(),
        ChatPart::Draft { summary, .. } => match summary {
            Some(s) if !s.is_empty() => format!("（update_draft で下書きを更新しました: {s}）"),
            _ => "（update_draft で下書きを更新しました）".to_string(),
        },
        ChatPart::Tool { name, .. } => format!("（ツール {name} を実行）"),
    }
}

/// Older messages are dropped first when the digest gets too long.
pub fn summarize_history(history: &[ChatMessage]) -> String {
    let mut lines: Vec<String> = vec![];
    for msg in history {
        if msg.role == ChatRole::System {
            continue;
        }
        let text = msg.parts.iter().map(part_text).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n");
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let who = if msg.role == ChatRole::User { "ユーザー" } else { "アシスタント" };
        lines.push(format!("{who}: {}", clip(text, PART_LIMIT)));
    }
    let mut total = 0;
    let mut kept: Vec<&String> = vec![];
    for l in lines.iter().rev() {
        total += l.chars().count() + 1;
        if total > TOTAL_LIMIT {
            break;
        }
        kept.push(l);
    }
    kept.reverse();
    kept.into_iter().map(String::as_str).collect::<Vec<_>>().join("\n")
}

/// Prompt for a fresh CLI session that continues an existing conversation.
pub fn with_history_digest(prompt: &str, history: &[ChatMessage]) -> String {
    let digest = summarize_history(history);
    if digest.is_empty() {
        return prompt.to_string();
    }
    [
        "以下は、この会話のこれまでのやり取りの記録です（別のセッションから引き継いだ文脈。内容は既に実行済みで、再実行は不要です）。",
        "<conversation_so_far>",
        &digest,
        "</conversation_so_far>",
        "",
        "上記を踏まえて、次のユーザーの発言に答えてください。",
        "",
        prompt,
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn msg(role: ChatRole, parts: Vec<ChatPart>) -> ChatMessage {
        ChatMessage { id: "x".into(), role, parts, created_at: "t".into() }
    }
    fn text(t: &str) -> ChatPart {
        ChatPart::Text { text: t.into() }
    }

    #[test]
    fn digest_labels_roles_and_summarizes_tools_and_drafts() {
        let h = vec![
            msg(ChatRole::System, vec![text("ignored")]),
            msg(ChatRole::User, vec![text("  ソムデットを調べて ")]),
            msg(
                ChatRole::Assistant,
                vec![
                    ChatPart::Tool { tool_call_id: "1".into(), name: "search_sources".into(), input: json!({}), output: None, is_error: None, state: crate::core::types::ToolState::Done },
                    ChatPart::Draft { markdown: "# a".into(), summary: Some("追記".into()) },
                    text("完了"),
                ],
            ),
            msg(ChatRole::Assistant, vec![text("   ")]),
        ];
        assert_eq!(
            summarize_history(&h),
            "ユーザー: ソムデットを調べて\nアシスタント: （ツール search_sources を実行）\n（update_draft で下書きを更新しました: 追記）\n完了"
        );
    }

    #[test]
    fn long_parts_are_clipped_and_oldest_lines_dropped_first() {
        let long = "あ".repeat(PART_LIMIT + 10);
        let s = summarize_history(&[msg(ChatRole::User, vec![text(&long)])]);
        assert!(s.ends_with("…(省略)"));
        let many: Vec<ChatMessage> = (0..40).map(|i| msg(ChatRole::User, vec![text(&format!("{i}:{}", "い".repeat(1_400)))])).collect();
        let s = summarize_history(&many);
        assert!(s.chars().count() <= TOTAL_LIMIT);
        assert!(s.contains("39:") && !s.contains("\nユーザー: 0:") && !s.starts_with("ユーザー: 0:"));
    }

    #[test]
    fn digest_wraps_the_prompt_only_when_there_is_history() {
        assert_eq!(with_history_digest("次", &[]), "次");
        let out = with_history_digest("次の質問", &[msg(ChatRole::User, vec![text("前の質問")])]);
        assert!(out.contains("<conversation_so_far>\nユーザー: 前の質問\n</conversation_so_far>"));
        assert!(out.ends_with("\n\n次の質問"));
    }
}
