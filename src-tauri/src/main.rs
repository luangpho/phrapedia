// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `phrapedia mcp ...` is the headless stdio MCP server (spawned by the in-app chat's `claude -p`, or by an
    // external MCP client). Decide before touching Tauri so that no window / WebView is ever created.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("mcp") {
        std::process::exit(phrapedia_lib::mcp::run_cli(&args[1..]));
    }
    phrapedia_lib::run()
}
