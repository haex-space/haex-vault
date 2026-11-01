//! The `--mcp-agent` helper (spec 021, contracts/agent-helper.md): an agent such as Claude Code or
//! Codex starts the holzi executable with this flag as its MCP server. The helper starts no Tauri,
//! no window and no vault; it finds the running holzi's endpoint, greets it with the agent's
//! credential, and then passes MCP lines through unchanged. It understands no MCP itself, except
//! for answering the first `initialize` with an error when holzi is not open or refuses the agent
//! (FR-002: it never starts holzi). Control lines from the app carry the credential after a
//! registration, which the helper keeps in a file only this user can read.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use uuid::Uuid;

use super::endpoint;
use super::hello::{
    json_line, read_control, valid_slug, CloseReason, Control, Greeting, HelloReply, RejectReason,
    HELLO_VERSION,
};

/// The flag that starts the helper instead of the app.
pub const FLAG: &str = "--mcp-agent";

/// Tauri's identifier (`tauri.conf.json`); the helper finds holzi's config directory with it.
pub const APP_IDENTIFIER: &str = "com.haex.holzi";

/// How long the helper waits for the app's reply to its greeting.
const REPLY_DEADLINE: Duration = Duration::from_secs(5);

/// How long the helper waits for the agent's first message when it only has an error to give.
const FIRST_MESSAGE_DEADLINE: Duration = Duration::from_secs(10);

/// The exit codes of contracts/agent-helper.md.
pub mod exit {
    /// stdin closed: the agent ended.
    pub const DONE: i32 = 0;
    pub const IO: i32 = 1;
    /// holzi is not open, or several instances are and none was chosen.
    pub const NOT_OPEN: i32 = 2;
    pub const REJECTED: i32 = 3;
    /// holzi ended the connection (revoked, vault closed, declined).
    pub const CLOSED: i32 = 4;
}

/// The arguments after [`FLAG`].
#[derive(Clone, PartialEq, Eq)]
pub struct HelperArgs {
    pub slug: String,
    /// The `vault_device_uuid` of the instance to reach, when several run.
    pub vault: Option<Uuid>,
    /// Set by holzi itself for a delegate; never read from or written to a file.
    pub delegate_token: Option<String>,
}

impl std::fmt::Debug for HelperArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HelperArgs")
            .field("slug", &self.slug)
            .field("vault", &self.vault)
            .field("delegate_token", &self.delegate_token.as_ref().map(|_| "…"))
            .finish()
    }
}

/// The executable an agent starts: the AppImage itself when holzi runs as one, else this binary.
pub fn executable() -> std::io::Result<String> {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return Ok(appimage.to_string_lossy().into_owned());
    }
    std::env::current_exe().map(|path| path.to_string_lossy().into_owned())
}

/// Reads `<slug> [--vault <id>] [--delegate-token <credential>]`.
pub fn parse_args(mut args: impl Iterator<Item = String>) -> Result<HelperArgs, String> {
    let slug = args.next().ok_or("missing the agent's slug")?;
    if !valid_slug(&slug) {
        return Err("the slug must match [a-z0-9-]{1,32}".to_owned());
    }
    let mut parsed = HelperArgs {
        slug,
        vault: None,
        delegate_token: None,
    };
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing the value of {flag}"))?;
        match flag.as_str() {
            "--vault" => {
                parsed.vault =
                    Some(Uuid::parse_str(&value).map_err(|_| "--vault needs a uuid".to_owned())?);
            }
            "--delegate-token" => parsed.delegate_token = Some(value),
            _ => return Err(format!("unknown argument {flag}")),
        }
    }
    Ok(parsed)
}

/// Where the helper looks for the endpoint and keeps credentials.
#[derive(Debug, Clone)]
pub struct HelperEnv {
    pub socket_dir: PathBuf,
    /// `<holzi config dir>/mcp-agents`.
    pub credential_dir: PathBuf,
}

impl HelperEnv {
    /// The directories of this user's holzi.
    pub fn from_system() -> Option<Self> {
        Some(Self {
            socket_dir: endpoint::default_socket_dir(),
            credential_dir: dirs::config_dir()?.join(APP_IDENTIFIER).join("mcp-agents"),
        })
    }
}

/// The file of the credential of `slug` for the endpoint `name`.
pub fn credential_path(env: &HelperEnv, name: &str, slug: &str) -> PathBuf {
    env.credential_dir.join(name).join(format!("{slug}.token"))
}

/// Runs the helper on `stdin`/`stdout` and returns its exit code.
pub async fn run<I, O>(args: HelperArgs, env: HelperEnv, stdin: I, mut stdout: O) -> i32
where
    I: AsyncRead + Unpin + Send + 'static,
    O: AsyncWrite + Unpin + Send,
{
    let mut stdin = BufReader::new(stdin);
    let name = match choose_endpoint(&env, args.vault) {
        Ok(name) => name,
        Err(message) => {
            answer_first_message(&mut stdin, &mut stdout, &message).await;
            return exit::NOT_OPEN;
        }
    };
    let Ok(stream) = endpoint::connect(&env.socket_dir, &name).await else {
        answer_first_message(&mut stdin, &mut stdout, "holzi is not open").await;
        return exit::NOT_OPEN;
    };
    let token_file = credential_path(&env, &name, &args.slug);
    let token = match &args.delegate_token {
        Some(token) => Some(token.clone()),
        None => read_credential(&token_file).await,
    };
    let greeting = Greeting {
        hello: HELLO_VERSION,
        slug: args.slug.clone(),
        token,
        delegate: args.delegate_token.is_some(),
    };
    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut from_app = BufReader::new(read_half);
    if write_half
        .write_all(json_line(&greeting).as_bytes())
        .await
        .is_err()
    {
        answer_first_message(&mut stdin, &mut stdout, "holzi is not open").await;
        return exit::NOT_OPEN;
    }
    let reply = read_reply(&mut from_app).await;
    match reply {
        Some(HelloReply::Accepted | HelloReply::Pending) => {}
        Some(HelloReply::Rejected { reason }) => {
            answer_first_message(&mut stdin, &mut stdout, &rejection_message(reason)).await;
            return exit::REJECTED;
        }
        None => {
            answer_first_message(&mut stdin, &mut stdout, "holzi did not answer").await;
            return exit::NOT_OPEN;
        }
    }
    let file = (args.delegate_token.is_none()).then_some(token_file);
    relay(stdin, stdout, from_app, write_half, file).await
}

/// The endpoint to reach: the one of `vault`, or the only one there is.
fn choose_endpoint(env: &HelperEnv, vault: Option<Uuid>) -> Result<String, String> {
    let names = endpoint::discover(&env.socket_dir);
    match vault {
        Some(vault) => {
            let name = endpoint::endpoint_name(vault);
            if names.contains(&name) {
                Ok(name)
            } else {
                Err("holzi is not open with this vault".to_owned())
            }
        }
        None => match names.as_slice() {
            [] => Err("holzi is not open".to_owned()),
            [only] => Ok(only.clone()),
            _ => Err(
                "several holzi instances are open; add --vault <id> from holzi's setup".to_owned(),
            ),
        },
    }
}

fn rejection_message(reason: RejectReason) -> String {
    match reason {
        RejectReason::Blocked => "holzi blocked this agent",
        RejectReason::RegistrationBusy => "holzi has too many open registrations; try again later",
        RejectReason::Disabled => "the MCP access of holzi is off",
        RejectReason::Malformed => "holzi could not read the greeting of this helper",
        RejectReason::NotRegistered => "this delegate has no rights in holzi; set them in the chat",
    }
    .to_owned()
}

async fn read_reply<R: AsyncBufReadExt + Unpin>(from_app: &mut R) -> Option<HelloReply> {
    let mut line = String::new();
    match tokio::time::timeout(REPLY_DEADLINE, from_app.read_line(&mut line)).await {
        Ok(Ok(read)) if read > 0 => serde_json::from_str(line.trim_end()).ok(),
        _ => None,
    }
}

async fn read_credential(path: &Path) -> Option<String> {
    let text = tokio::fs::read_to_string(path).await.ok()?;
    let token = text.trim();
    (!token.is_empty()).then(|| token.to_owned())
}

/// Saves the credential readable only by this user.
async fn write_credential(path: &Path, credential: &str) -> std::io::Result<()> {
    let path = path.to_owned();
    let credential = credential.to_owned();
    tokio::task::spawn_blocking(move || {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
            }
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        use std::io::Write as _;
        let mut file = options.open(&path)?;
        file.write_all(credential.as_bytes())?;
        file.sync_all()
    })
    .await
    .map_err(|error| std::io::Error::other(error.to_string()))?
}

/// Answers the agent's first request with a JSON-RPC error, so the agent shows `message`.
async fn answer_first_message<I, O>(stdin: &mut BufReader<I>, stdout: &mut O, message: &str)
where
    I: AsyncRead + Unpin,
    O: AsyncWrite + Unpin,
{
    let mut line = String::new();
    let read = tokio::time::timeout(FIRST_MESSAGE_DEADLINE, stdin.read_line(&mut line)).await;
    if !matches!(read, Ok(Ok(read)) if read > 0) {
        return;
    }
    let Ok(request) = serde_json::from_str::<Value>(line.trim_end()) else {
        return;
    };
    let Some(id) = request.get("id").cloned() else {
        return;
    };
    let answer = json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": -32000, "message": message },
    });
    let _ = stdout.write_all(json_line(&answer).as_bytes()).await;
    let _ = stdout.flush().await;
}

/// Passes MCP lines both ways until one side ends; keeps or deletes the credential on a control
/// line.
async fn relay<I, O, R, W>(
    mut stdin: BufReader<I>,
    mut stdout: O,
    mut from_app: BufReader<R>,
    mut to_app: W,
    token_file: Option<PathBuf>,
) -> i32
where
    I: AsyncRead + Unpin + Send + 'static,
    O: AsyncWrite + Unpin + Send,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let upstream = tokio::spawn(async move {
        let copied = tokio::io::copy_buf(&mut stdin, &mut to_app).await;
        let _ = to_app.shutdown().await;
        copied.is_ok()
    });
    let mut closed_by_app = false;
    let mut line = Vec::new();
    loop {
        line.clear();
        match from_app.read_until(b'\n', &mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let content = line.strip_suffix(b"\n").unwrap_or(&line);
        match read_control(content) {
            Some(Control::Token(credential)) => {
                if let Some(file) = &token_file {
                    if let Err(error) = write_credential(file, &credential).await {
                        eprintln!("holzi: could not keep the credential: {error}");
                    }
                }
            }
            Some(Control::Closing(reason)) => {
                closed_by_app = true;
                if reason == CloseReason::Revoked {
                    if let Some(file) = &token_file {
                        let _ = tokio::fs::remove_file(file).await;
                    }
                }
            }
            None => {
                if stdout.write_all(&line).await.is_err() || stdout.flush().await.is_err() {
                    return exit::IO;
                }
            }
        }
    }
    let agent_ended = upstream.is_finished();
    upstream.abort();
    if agent_ended && !closed_by_app {
        exit::DONE
    } else {
        exit::CLOSED
    }
}

/// The helper as the process entry point: stdin and stdout of this process.
pub fn run_process(args: impl Iterator<Item = String>) -> i32 {
    let args = match parse_args(args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("holzi {FLAG}: {message}");
            return exit::IO;
        }
    };
    let Some(env) = HelperEnv::from_system() else {
        eprintln!("holzi {FLAG}: no config directory");
        return exit::IO;
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("holzi {FLAG}: {error}");
            return exit::IO;
        }
    };
    let code = runtime.block_on(run(args, env, tokio::io::stdin(), tokio::io::stdout()));
    // stdin uses an uncancellable blocking read; do not wait for it while dropping the runtime.
    runtime.shutdown_background();
    code
}
