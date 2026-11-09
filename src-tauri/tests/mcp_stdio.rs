//! End-to-end: the real `phrapedia mcp` subcommand over stdio (initialize → tools/list → tools/call → resources → prompts).
//! Runs the compiled binary, so it also proves that `mcp` never starts the GUI (it would hang / fail without a display).

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct Client {
    child: Child,
    stdin: Option<ChildStdin>,
    out: BufReader<ChildStdout>,
    next_id: u64,
}

impl Client {
    fn start(config_dir: &std::path::Path, data_dir: &std::path::Path) -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_phrapedia"))
            .args(["mcp", "--config-dir"])
            .arg(config_dir)
            .arg("--data-dir")
            .arg(data_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn phrapedia mcp");
        let stdin = child.stdin.take();
        let out = BufReader::new(child.stdout.take().unwrap());
        Client { child, stdin, out, next_id: 1 }
    }

    fn send(&mut self, v: Value) {
        let s = self.stdin.as_mut().unwrap();
        writeln!(s, "{v}").unwrap();
        s.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let mut line = String::new();
            assert!(self.out.read_line(&mut line).unwrap() > 0, "server closed stdout while waiting for {method}");
            let v: Value = serde_json::from_str(line.trim()).unwrap_or_else(|e| panic!("non-JSON on stdout: {line:?} ({e})"));
            if v["id"] == json!(id) {
                return v;
            }
        }
    }

    fn call(&mut self, name: &str, args: Value) -> Value {
        self.request("tools/call", json!({"name": name, "arguments": args}))["result"].clone()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn text_of(result: &Value) -> String {
    result["content"][0]["text"].as_str().unwrap_or_else(|| panic!("no text content: {result}")).to_string()
}

const MONK: &str = "---\nslug: ''\nkind: monks\nname: ソムデット\nname_en: Somdet To\nstatus: draft\nsources: []\ntags: []\n---\n# ソムデット\n";

#[test]
fn initialize_list_call_resources_and_prompts() {
    let tmp = tempfile::tempdir().unwrap();
    let (config, data, dataset) = (tmp.path().join("config"), tmp.path().join("data"), tmp.path().join("dataset"));
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(dataset.join("sources/notes")).unwrap();
    std::fs::write(config.join("settings.json"), json!({"datasetPath": dataset}).to_string()).unwrap();
    std::fs::write(dataset.join("sources/notes/a.md"), "# ソムデット\nวัดระฆังโฆสิตาราม\n").unwrap();

    let mut c = Client::start(&config, &data);

    let init = c.request("initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}));
    assert_eq!(init["result"]["serverInfo"]["name"], "phrapedia");
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    let caps = &init["result"]["capabilities"];
    assert!(caps["tools"].is_object() && caps["resources"].is_object() && caps["prompts"].is_object(), "{caps}");
    c.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    // tools/list
    let list = c.request("tools/list", json!({}));
    let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        ["list_entities", "get_entity", "entity_template", "save_entity", "list_sources", "read_source", "search_sources", "web_research", "sync_status", "sync_data", "update_draft"]
    );
    let save = list["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "save_entity").unwrap();
    assert_eq!(save["inputSchema"]["required"], json!(["kind", "markdown"]));
    assert!(save["description"].as_str().unwrap().starts_with("Write a record as a markdown file"));

    // tools/call: list_entities (empty) → save_entity → get_entity → search_sources
    let r = c.call("list_entities", json!({"kind": "monks"}));
    let v: Value = serde_json::from_str(&text_of(&r)).unwrap();
    assert_eq!((v["kind"].as_str(), v["count"].as_u64()), (Some("monks"), Some(0)));
    assert!(r["isError"].is_null() || r["isError"] == false);

    let saved: Value = serde_json::from_str(&text_of(&c.call("save_entity", json!({"kind": "monks", "markdown": MONK})))).unwrap();
    assert_eq!((saved["ok"].as_bool(), saved["slug"].as_str()), (Some(true), Some("somdet-to")));
    assert!(dataset.join("data/monks/somdet-to.md").is_file(), "Markdown is the source of truth");

    let got: Value = serde_json::from_str(&text_of(&c.call("get_entity", json!({"kind": "monks", "slug": "somdet-to"})))).unwrap();
    assert_eq!(got["name"], "ソムデット");
    let hits: Value = serde_json::from_str(&text_of(&c.call("search_sources", json!({"query": "วัดระฆัง"})))).unwrap();
    assert_eq!(hits["hits"][0]["path"], "notes/a.md");

    // tool failure is an isError result carrying the message, not a protocol error
    let bad = c.call("get_entity", json!({"kind": "monks", "slug": "missing"}));
    assert_eq!(bad["isError"], true);
    assert_eq!(text_of(&bad), "monks/missing not found");

    // update_draft is an ack only
    assert_eq!(text_of(&c.call("update_draft", json!({"markdown": "# x"}))), "draft updated (not saved yet)");

    // unknown tool → JSON-RPC error
    let unknown = c.request("tools/call", json!({"name": "nope", "arguments": {}}));
    assert!(unknown["error"].is_object(), "{unknown}");

    // resources
    let res = c.request("resources/list", json!({}));
    assert_eq!(res["result"]["resources"][0]["uri"], "phrapedia://monks/somdet-to");
    let tpl = c.request("resources/templates/list", json!({}));
    assert_eq!(tpl["result"]["resourceTemplates"][0]["uriTemplate"], "phrapedia://{kind}/{slug}");
    let read = c.request("resources/read", json!({"uri": "phrapedia://monks/somdet-to"}));
    assert!(read["result"]["contents"][0]["text"].as_str().unwrap().contains("# ソムデット"));
    assert_eq!(read["result"]["contents"][0]["mimeType"], "text/markdown");

    // prompts
    let prompts = c.request("prompts/list", json!({}));
    assert_eq!(prompts["result"]["prompts"][0]["name"], "research_record");
    let p = c.request("prompts/get", json!({"name": "research_record", "arguments": {"kind": "temples", "name": "ワット・X"}}));
    assert!(p["result"]["messages"][0]["content"]["text"].as_str().unwrap().starts_with("「ワット・X」という寺院のレコードを作成してください"));
}

#[test]
fn exits_cleanly_when_stdin_closes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut c = Client::start(&tmp.path().join("c"), &tmp.path().join("d"));
    c.request("initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}));
    drop(c.stdin.take());
    let status = c.child.wait().unwrap();
    assert!(status.success(), "{status:?}");
}

#[test]
fn bad_arguments_exit_with_usage() {
    let out = Command::new(env!("CARGO_BIN_EXE_phrapedia")).args(["mcp", "--bogus"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage: phrapedia mcp"));
    assert!(out.stdout.is_empty(), "stdout is the protocol channel");
}
