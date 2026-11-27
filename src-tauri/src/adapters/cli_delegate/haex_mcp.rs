//! haex-vault's own MCP server for a delegate in the chat (spec 021 US6, FR-024, research R7): a
//! registered Claude Code or Codex gets the server `haex` in its temporary configuration on every
//! start, in every autonomy mode, without the user setting anything up. The server is haex-vault's
//! `--mcp-agent` helper with a pass that lives only for this start. haex-vault checks each of its tools
//! against the delegate's rights and approval mode itself, so the delegate's own approval lets them
//! through: "Ungated" (009) never reaches them.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{json, Map, Value};
use uuid::Uuid;

use super::{DelegateChatContext, DelegateVendor};
use crate::adapters::AdapterError;

/// The name of haex-vault's server in the delegate's configuration.
pub const SERVER_NAME: &str = "haex";
/// Claude's permission rule for every tool of [`SERVER_NAME`]: haex-vault asks for them itself.
pub const CLAUDE_HAEX_TOOLS: &str = "mcp__haex";

/// What a delegate with haex-vault's server reads on top of the chat's system prompt: haex-vault's tools come
/// before its own web tools, and a request for an app's data is looked up in haex-vault's tool list.
pub const DELEGATE_INSTRUCTION: &str = "You work inside haex and have its tools through the MCP \
    server `haex` (tool names start with `mcp__haex__` or are listed under `haex`). Use them \
    before your own tools. Research on the web, searching a shop or opening a website: use \
    haex's agent browser (`browser_search`, `browser_open`, `browser_follow_link`, \
    `browser_page_read`), not your own web fetch or web search; if it fails, say what failed, \
    never that the website is down. Reading mails, calendar, notes or any other data of the user: \
    look through the tools of `haex` for one of an extension that fits (extension tools are \
    named after the extension) and use it; only when none fits, say that no installed extension \
    can do it. Never claim you cannot use haex tools without having looked at the list.";

/// What Abacus reads on every start about its own web tools, which run on its server where no rule
/// of the program reaches them (spec 102 FR-009, research R4): a request, not a block.
pub const ABACUS_WEB_INSTRUCTION: &str = "Do not use your own web search \
    (`web_search_text_only`, shown as WebSearch) or your own page fetch (`scrape_url_content`, \
    shown as Fetch).";

/// What Abacus reads after [`ABACUS_WEB_INSTRUCTION`] when haex-vault's server is not there for
/// this start.
pub const ABACUS_WEB_WITHOUT_HAEX: &str = "If a task needs the web, say that haex's agent \
    browser is not available for this answer.";

/// `system_prompt` with [`DELEGATE_INSTRUCTION`] when the delegate has haex-vault's server, and,
/// for Abacus, the sentence about its own web tools on every start.
pub fn with_instruction(
    system_prompt: Option<&str>,
    vendor: DelegateVendor,
    haex: bool,
) -> Option<String> {
    let mut parts: Vec<&str> = system_prompt
        .filter(|given| !given.trim().is_empty())
        .into_iter()
        .collect();
    if haex {
        parts.push(DELEGATE_INSTRUCTION);
    }
    if vendor == DelegateVendor::Abacus {
        parts.push(ABACUS_WEB_INSTRUCTION);
        if !haex {
            parts.push(ABACUS_WEB_WITHOUT_HAEX);
        }
    }
    if parts.is_empty() {
        // A given prompt of only whitespace stays as it was given.
        return system_prompt.map(str::to_owned);
    }
    Some(parts.join("\n\n"))
}

/// How a delegate starts haex-vault's server. Its arguments carry the pass, so `Debug` hides them; the
/// pass ends when this value is dropped, together with the delegate's start.
pub struct HaexServer {
    pub command: String,
    pub args: Vec<String>,
    _pass: Box<dyn Send + Sync>,
}

impl HaexServer {
    pub fn new(command: String, args: Vec<String>, pass: impl Send + Sync + 'static) -> Self {
        Self {
            command,
            args,
            _pass: Box::new(pass),
        }
    }
}

impl std::fmt::Debug for HaexServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HaexServer")
            .field("command", &self.command)
            .finish_non_exhaustive()
    }
}

/// Whether and how a delegate reaches haex-vault; the MCP server answers it (`mcp_server::delegates`).
#[async_trait]
pub trait HaexAccess: Send + Sync {
    /// haex-vault's server for a start of `vendor` with the model `model` (`<provider id>:<remote id>`) in
    /// the chat thread `thread`, or why there is none for it (the server does not run, the active agent
    /// profile does not offer the model, no pass could be made). What the delegate may do is the
    /// profile of that model.
    async fn server_for(
        &self,
        vendor: DelegateVendor,
        thread: Option<Uuid>,
        model: &str,
    ) -> Result<HaexServer, String>;
}

/// The event that tells the chat a delegate's start has no haex-vault tools, and why (spec 102
/// FR-012).
pub const EVENT_TOOLS_UNAVAILABLE: &str = "chat-delegate-tools-unavailable";

/// haex-vault's server for this start, if the context has the access (a job has none, spec 055).
/// A chat start without it tells the chat why (spec 102 FR-012) and runs without haex-vault's tools;
/// its own web tools stay blocked.
pub async fn server_for(
    context: &DelegateChatContext,
    vendor: DelegateVendor,
    thread: Option<Uuid>,
    model: &str,
) -> Option<HaexServer> {
    let access = context.haex.as_ref()?;
    match access.server_for(vendor, thread, model).await {
        Ok(server) => Some(server),
        Err(reason) => {
            if let Some(thread) = thread {
                (context.emit)(
                    EVENT_TOOLS_UNAVAILABLE,
                    json!({
                        "threadId": thread,
                        "delegateKind": crate::mcp_server::delegates::kind_of(vendor),
                        "reason": reason,
                    }),
                );
            }
            None
        }
    }
}

/// The approval bridge Claude asks about its own tools outside "Ungated" (spec 009): this
/// executable and the bridge's socket.
pub struct ApproveBridge<'a> {
    pub executable: &'a Path,
    pub socket: &'a Path,
}

/// Claude's `mcp-config.json`: `haex-approve` for the approval bridge, and `haex` where the
/// delegate is registered. `None` when neither applies.
pub fn claude_config(
    approve: Option<ApproveBridge<'_>>,
    haex: Option<&HaexServer>,
) -> Option<Value> {
    let mut servers = Map::new();
    if let Some(ApproveBridge { executable, socket }) = approve {
        servers.insert(
            "haex-approve".to_owned(),
            json!({
                "command": executable,
                "args": ["--internal-cli-delegate-approval-bridge", "--socket", socket],
            }),
        );
    }
    if let Some(haex) = haex {
        servers.insert(
            SERVER_NAME.to_owned(),
            json!({ "command": haex.command, "args": haex.args }),
        );
    }
    (!servers.is_empty()).then(|| json!({ "mcpServers": servers }))
}

/// Writes Claude's `mcp-config.json` into `dir`; its path, or `None` when there is nothing to write.
pub async fn write_claude_config(
    dir: &Path,
    approve_socket: Option<&Path>,
    haex: Option<&HaexServer>,
) -> Result<Option<PathBuf>, AdapterError> {
    let executable = match approve_socket {
        Some(_) => Some(std::env::current_exe().map_err(|error| AdapterError::Http {
            reason: format!("failed to locate HaexVault executable for approval bridge: {error}"),
        })?),
        None => None,
    };
    let approve = approve_socket
        .zip(executable.as_deref())
        .map(|(socket, executable)| ApproveBridge { executable, socket });
    let Some(config) = claude_config(approve, haex) else {
        return Ok(None);
    };
    let path = dir.join("mcp-config.json");
    let bytes = serde_json::to_vec(&config).map_err(|error| AdapterError::Http {
        reason: format!("failed to serialize Claude MCP config: {error}"),
    })?;
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|error| AdapterError::Http {
            reason: format!("failed to write Claude MCP config: {error}"),
        })?;
    Ok(Some(path))
}

/// The `config.toml` of `CODEX_HOME` with haex-vault's server. Codex asks the client before every call
/// of an MCP tool (`mcpServer/elicitation/request`) unless the server's approval mode is
/// `approve`; haex-vault checks its tools against the delegate's rights and the chat's approval mode
/// itself, so the delegate's own question is switched off for this server and for no other. Codex
/// accepts `auto`, `prompt`, `writes` and `approve` (measured with codex 0.169.0).
pub fn codex_config(haex: &HaexServer) -> String {
    let args: Vec<String> = haex.args.iter().map(|arg| toml_string(arg)).collect();
    format!(
        "[mcp_servers.{SERVER_NAME}]\ncommand = {}\nargs = [{}]\ndefault_tools_approval_mode = \"approve\"\n",
        toml_string(&haex.command),
        args.join(", ")
    )
}

/// Writes `config.toml` into Codex's home `home` when the delegate gets haex-vault's server.
pub async fn write_codex_config(
    home: &Path,
    haex: Option<&HaexServer>,
) -> Result<(), AdapterError> {
    let Some(haex) = haex else {
        return Ok(());
    };
    tokio::fs::write(home.join("config.toml"), codex_config(haex))
        .await
        .map_err(|error| AdapterError::Http {
            reason: format!("failed to write codex config.toml: {error}"),
        })
}

/// A TOML basic string: quotes, backslashes and control characters escaped.
fn toml_string(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for c in text.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            c if c.is_control() => quoted.push_str(&format!("\\u{:04X}", u32::from(c))),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}
