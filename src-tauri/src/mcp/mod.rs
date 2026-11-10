//! `phrapedia mcp`: stdio MCP server (same binary, no window). Port of server/mcp/**.
pub mod server;
pub mod tools;

pub use server::run_cli;
