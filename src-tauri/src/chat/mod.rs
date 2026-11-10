//! In-app chat: `claude -p` runner, stream-json → `ChatStreamEvent` conversion, system prompt, turn orchestration.
//! Tauri-independent (the command layer lives in `crate::commands::chat`). Port of server/utils/agent/*.

pub mod claude;
pub mod history;
pub mod prompt;
pub mod stream;
pub mod turn;

/// Name under which the app's own MCP server is registered in `--mcp-config`. Tools show up as `mcp__phrapedia__<name>`.
pub const MCP_SERVER_NAME: &str = "phrapedia";
pub const MCP_TOOL_PREFIX: &str = "mcp__phrapedia__";
