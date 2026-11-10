//! holzi's own MCP server for a delegate in the chat (spec 021 US6, FR-024, research R7): a
//! registered Claude Code or Codex gets the server `holzi` in its temporary configuration on every
//! start, in every autonomy mode, without the user setting anything up. The server is holzi's
//! `--mcp-agent` helper with a pass that lives only for this start. holzi checks each of its tools
//! against the delegate's rights and approval mode itself, so the delegate's own approval lets them
//! through: "Ungated" (009) never reaches them.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{json, Map, Value};
use uuid::Uuid;

use super::{DelegateChatContext, DelegateVendor};
use crate::adapters::AdapterError;

/// The name of holzi's server in the delegate's configuration.
pub const SERVER_NAME: &str = "holzi";
/// Claude's permission rule for every tool of [`SERVER_NAME`]: holzi asks for them itself.
pub const CLAUDE_HOLZI_TOOLS: &str = "mcp__holzi";

/// What a delegate with holzi's server reads on top of the chat's system prompt: holzi's tools come
/// before its own web tools, and a request for an app's data is looked up in holzi's tool list.
pub const DELEGATE_INSTRUCTION: &str = "You work inside haex and have its tools through the MCP \
    server `holzi` (tool names start with `mcp__holzi__` or are listed under `holzi`). Use them \
    before your own tools. Research on the web, searching a shop or opening a website: use \
    haex's agent browser (`browser_search`, `browser_open`, `browser_follow_link`, \
    `browser_page_read`), not your own web fetch or web search; if it fails, say what failed, \
    never that the website is down. Reading mails, calendar, notes or any other data of the user: \
    look through the tools of `holzi` for one of an extension that fits (extension tools are \
    named after the extension) and use it; only when none fits, say that no installed extension \
    can do it. Never claim you cannot use haex tools without having looked at the list.";

/// `system_prompt` with [`DELEGATE_INSTRUCTION`] when the delegate has holzi's server.
pub fn with_instruction(system_prompt: Option<&str>, holzi: bool) -> Option<String> {
    if !holzi {
        return system_prompt.map(str::to_owned);
    }
    Some(
        match system_prompt.filter(|given| !given.trim().is_empty()) {
            Some(given) => format!("{given}\n\n{DELEGATE_INSTRUCTION}"),
            None => DELEGATE_INSTRUCTION.to_owned(),
        },
    )
}

/// How a delegate starts holzi's server. Its arguments carry the pass, so `Debug` hides them; the
/// pass ends when this value is dropped, together with the delegate's start.
pub struct HolziServer {
    pub command: String,
    pub args: Vec<String>,
    _pass: Box<dyn Send + Sync>,
}

impl HolziServer {
    pub fn new(command: String, args: Vec<String>, pass: impl Send + Sync + 'static) -> Self {
        Self {
            command,
            args,
            _pass: Box::new(pass),
        }
    }
}

impl std::fmt::Debug for HolziServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HolziServer")
            .field("command", &self.command)
            .finish_non_exhaustive()
    }
}

/// Whether and how a delegate reaches holzi; the MCP server answers it (`mcp_server::delegates`).
#[async_trait]
pub trait HolziAccess: Send + Sync {
    /// holzi's server for a start of `vendor` with the model `model` (`<provider id>:<remote id>`) in
    /// the chat thread `thread`; `None` while the access is off or the active agent profile does not
    /// offer the model. What the delegate may do is the profile of that model.
    async fn server_for(
        &self,
        vendor: DelegateVendor,
        thread: Option<Uuid>,
        model: &str,
    ) -> Option<HolziServer>;
}

/// holzi's server for this start, if the context has the access (a job has none, spec 055).
pub async fn server_for(
    context: &DelegateChatContext,
    vendor: DelegateVendor,
    thread: Option<Uuid>,
    model: &str,
) -> Option<HolziServer> {
    match &context.holzi {
        Some(access) => access.server_for(vendor, thread, model).await,
        None => None,
    }
}

/// The approval bridge Claude asks about its own tools outside "Ungated" (spec 009): this
/// executable and the bridge's socket.
pub struct ApproveBridge<'a> {
    pub executable: &'a Path,
    pub socket: &'a Path,
}

/// Claude's `mcp-config.json`: `holzi-approve` for the approval bridge, and `holzi` where the
/// delegate is registered. `None` when neither applies.
pub fn claude_config(
    approve: Option<ApproveBridge<'_>>,
    holzi: Option<&HolziServer>,
) -> Option<Value> {
    let mut servers = Map::new();
    if let Some(ApproveBridge { executable, socket }) = approve {
        servers.insert(
            "holzi-approve".to_owned(),
            json!({
                "command": executable,
                "args": ["--internal-cli-delegate-approval-bridge", "--socket", socket],
            }),
        );
    }
    if let Some(holzi) = holzi {
        servers.insert(
            SERVER_NAME.to_owned(),
            json!({ "command": holzi.command, "args": holzi.args }),
        );
    }
    (!servers.is_empty()).then(|| json!({ "mcpServers": servers }))
}

/// Writes Claude's `mcp-config.json` into `dir`; its path, or `None` when there is nothing to write.
pub async fn write_claude_config(
    dir: &Path,
    approve_socket: Option<&Path>,
    holzi: Option<&HolziServer>,
) -> Result<Option<PathBuf>, AdapterError> {
    let executable = match approve_socket {
        Some(_) => Some(std::env::current_exe().map_err(|error| AdapterError::Http {
            reason: format!("failed to locate holzi executable for approval bridge: {error}"),
        })?),
        None => None,
    };
    let approve = approve_socket
        .zip(executable.as_deref())
        .map(|(socket, executable)| ApproveBridge { executable, socket });
    let Some(config) = claude_config(approve, holzi) else {
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

/// The `config.toml` of `CODEX_HOME` with holzi's server.
pub fn codex_config(holzi: &HolziServer) -> String {
    let args: Vec<String> = holzi.args.iter().map(|arg| toml_string(arg)).collect();
    format!(
        "[mcp_servers.{SERVER_NAME}]\ncommand = {}\nargs = [{}]\n",
        toml_string(&holzi.command),
        args.join(", ")
    )
}

/// Writes `config.toml` into Codex's home `home` when the delegate gets holzi's server.
pub async fn write_codex_config(
    home: &Path,
    holzi: Option<&HolziServer>,
) -> Result<(), AdapterError> {
    let Some(holzi) = holzi else {
        return Ok(());
    };
    tokio::fs::write(home.join("config.toml"), codex_config(holzi))
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
