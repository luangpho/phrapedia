//! rmcp adapter: serves `tools.rs` over stdio. Entry: `phrapedia mcp --config-dir <dir> --data-dir <dir>`.

use super::tools::{self, Content};
use crate::core::{AppDirs, Core};
use rmcp::model::*;
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt};
use serde_json::Value;
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct PhrapediaServer {
    core: Arc<Core>,
}

impl PhrapediaServer {
    pub fn new(core: Arc<Core>) -> Self {
        PhrapediaServer { core }
    }
}

fn tool_defs() -> Vec<Tool> {
    tools::specs()
        .into_iter()
        .map(|s| {
            let schema = match s.schema {
                Value::Object(o) => o,
                _ => Default::default(),
            };
            let mut ann = ToolAnnotations::with_title(s.title).read_only(s.read_only);
            if let Some(d) = s.destructive {
                ann = ann.destructive(d);
            }
            if let Some(i) = s.idempotent {
                ann = ann.idempotent(i);
            }
            if let Some(o) = s.open_world {
                ann = ann.open_world(o);
            }
            Tool::new(s.name, s.description, std::sync::Arc::new(schema)).with_title(s.title).with_annotations(ann)
        })
        .collect()
}

fn to_block(c: Content) -> ContentBlock {
    match c {
        Content::Text(t) => ContentBlock::text(t),
        Content::Image { data, mime_type } => ContentBlock::image(data, mime_type),
    }
}

impl ServerHandler for PhrapediaServer {
    fn get_info(&self) -> ServerConfig {
        let caps = ServerCapabilities::builder().enable_tools().enable_resources().enable_prompts().build();
        let mut info = ServerConfig::new(caps).with_instructions(
            "Phrapedia: Thai Buddhist monks / temples / amulets knowledge base. Records are Markdown (source of truth); primary sources are read-only. \
             Search primary sources first (search_sources / read_source), use web_research only for gaps, and save with save_entity.",
        );
        info.server_info = Implementation::new("phrapedia", env!("CARGO_PKG_VERSION"));
        info
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        // Clients in the wild (Claude Code) speak the initialize-based revisions; do not advertise newer ones.
        Cow::Borrowed(ProtocolVersion::known_up_to(&ProtocolVersion::V_2025_11_25))
    }

    async fn list_tools(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(tool_defs()))
    }

    async fn call_tool(&self, request: CallToolRequestParams, _context: RequestContext<RoleServer>) -> Result<CallToolResponse, McpError> {
        let name = request.name.to_string();
        let args = request.arguments.unwrap_or_default();
        let core = self.core.clone();
        let outcome = tokio::task::spawn_blocking(move || tools::call(&core, &name, &args).map(|r| (name, r)))
            .await
            .map_err(|e| McpError::internal_error(format!("tool task failed: {e}"), None))?;
        match outcome {
            None => Err(McpError::invalid_params(format!("unknown tool: {}", request.name), None)),
            Some((_, Ok(content))) => Ok(CallToolResult::success(content.into_iter().map(to_block).collect()).into()),
            Some((_, Err(message))) => Ok(CallToolResult::error(vec![ContentBlock::text(message)]).into()),
        }
    }

    async fn list_resources(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> Result<ListResourcesResult, McpError> {
        let core = self.core.clone();
        let items = tokio::task::spawn_blocking(move || tools::list_resources(&core)).await.map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let resources = items.into_iter().map(|(uri, name)| Resource::new(uri, name).with_mime_type("text/markdown")).collect();
        Ok(ListResourcesResult::with_all_items(resources))
    }

    async fn list_resource_templates(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> Result<ListResourceTemplatesResult, McpError> {
        let t = ResourceTemplate::new(tools::RESOURCE_TEMPLATE, "entity")
            .with_title("Phrapedia record")
            .with_description("Markdown record of a monk, temple or amulet: phrapedia://{kind}/{slug}")
            .with_mime_type("text/markdown");
        Ok(ListResourceTemplatesResult::with_all_items(vec![t]))
    }

    async fn read_resource(&self, request: ReadResourceRequestParams, _context: RequestContext<RoleServer>) -> Result<ReadResourceResponse, McpError> {
        let core = self.core.clone();
        let uri = request.uri.clone();
        let r = tokio::task::spawn_blocking(move || tools::read_resource(&core, &uri)).await.map_err(|e| McpError::internal_error(e.to_string(), None))?;
        match r {
            Ok(md) => Ok(ReadResourceResult::new(vec![ResourceContents::text(md, request.uri).with_mime_type("text/markdown")]).into()),
            Err(tools::ResourceError::NotFound(m)) => Err(McpError::resource_not_found(m, None)),
            Err(tools::ResourceError::Invalid(m)) => Err(McpError::invalid_params(m, None)),
        }
    }

    async fn list_prompts(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> Result<ListPromptsResult, McpError> {
        let args = vec![
            PromptArgument::new("kind").with_description("monks | temples").with_required(true),
            PromptArgument::new("name").with_description("Name of the monk or temple (any language)").with_required(true),
        ];
        let p = Prompt::new(tools::PROMPT_NAME, Some("Guided workflow: gather primary sources, fill gaps with web research, and save a monk or temple record."), Some(args)).with_title("Research and write a record");
        Ok(ListPromptsResult::with_all_items(vec![p]))
    }

    async fn get_prompt(&self, request: GetPromptRequestParams, _context: RequestContext<RoleServer>) -> Result<GetPromptResponse, McpError> {
        if request.name != tools::PROMPT_NAME {
            return Err(McpError::invalid_params(format!("unknown prompt: {}", request.name), None));
        }
        let text = tools::research_record_prompt(&request.arguments.unwrap_or_default()).map_err(|m| McpError::invalid_params(m, None))?;
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(Role::User, text)]).with_description("Research and write a record").into())
    }
}

/// `phrapedia mcp [--config-dir <dir>] [--data-dir <dir>]`: parse flags, serve MCP on stdin/stdout until the client closes it.
/// Returns the process exit code. Logs go to stderr only (stdout is the protocol channel).
pub fn run_cli(args: &[String]) -> i32 {
    let dirs = match parse_args(args, AppDirs::from_os()) {
        Ok(d) => d,
        Err(msg) => {
            eprintln!("phrapedia mcp: {msg}\nusage: phrapedia mcp [--config-dir <dir>] [--data-dir <dir>]");
            return 2;
        }
    };
    let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("phrapedia mcp: cannot start runtime: {e}");
            return 1;
        }
    };
    let core = Arc::new(Core::new(dirs));
    let res: Result<(), String> = rt.block_on(async move {
        let service = PhrapediaServer::new(core).serve(rmcp::transport::stdio()).await.map_err(|e| e.to_string())?;
        service.waiting().await.map_err(|e| e.to_string())?;
        Ok(())
    });
    match res {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("phrapedia mcp: {e}");
            1
        }
    }
}

/// `--config-dir` / `--data-dir` (also `--flag=value`); omitted ones fall back to the OS-standard location.
pub fn parse_args(args: &[String], defaults: AppDirs) -> Result<AppDirs, String> {
    let mut dirs = defaults;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) => (f, Some(v.to_string())),
            None => (a.as_str(), None),
        };
        let mut value = || inline.clone().or_else(|| it.next().cloned()).filter(|v| !v.is_empty()).ok_or_else(|| format!("{flag} needs a value"));
        match flag {
            "--config-dir" => dirs.config_dir = PathBuf::from(value()?),
            "--data-dir" => dirs.data_dir = PathBuf::from(value()?),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(dirs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }
    fn base() -> AppDirs {
        AppDirs { config_dir: "/os/c".into(), data_dir: "/os/d".into() }
    }

    #[test]
    fn args_override_os_defaults() {
        let d = parse_args(&s(&["--config-dir", "/a", "--data-dir=/b"]), base()).unwrap();
        assert_eq!((d.config_dir, d.data_dir), (PathBuf::from("/a"), PathBuf::from("/b")));
        let d = parse_args(&[], base()).unwrap();
        assert_eq!((d.config_dir, d.data_dir), (PathBuf::from("/os/c"), PathBuf::from("/os/d")));
        let d = parse_args(&s(&["--data-dir", "/only"]), base()).unwrap();
        assert_eq!((d.config_dir, d.data_dir), (PathBuf::from("/os/c"), PathBuf::from("/only")));
    }

    #[test]
    fn bad_args_are_rejected() {
        assert!(parse_args(&s(&["--config-dir"]), base()).unwrap_err().contains("needs a value"));
        assert!(parse_args(&s(&["--nope", "x"]), base()).unwrap_err().contains("unknown argument"));
    }

    #[test]
    fn tool_defs_carry_schema_and_annotations() {
        let defs = tool_defs();
        assert_eq!(defs.len(), tools::TOOL_NAMES.len());
        let save = defs.iter().find(|t| t.name == "save_entity").unwrap();
        assert_eq!(save.input_schema["required"], serde_json::json!(["kind", "markdown"]));
        let a = save.annotations.as_ref().unwrap();
        assert_eq!((a.read_only_hint, a.destructive_hint, a.idempotent_hint), (Some(false), Some(true), Some(true)));
        let status = defs.iter().find(|t| t.name == "sync_status").unwrap();
        assert_eq!(status.annotations.as_ref().unwrap().read_only_hint, Some(true));
    }
}
