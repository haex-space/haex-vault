//! The greeting between the `--mcp-agent` helper and the app (spec 021, contracts/agent-helper.md):
//! one JSON line each way before the first MCP byte, and afterwards the control lines the app slips
//! between MCP messages towards the helper. MCP over stdio is one JSON message per line, and a
//! control line is an object with the single key `holzi_control`, which no JSON-RPC message has.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The version of this greeting.
pub const HELLO_VERSION: u32 = 1;

/// The longest greeting line holzi reads (contracts/agent-helper.md, "Grenzen").
pub const MAX_GREETING_BYTES: usize = 4 * 1024;

/// The longest slug: `[a-z0-9-]{1,32}`.
pub const MAX_SLUG_CHARS: usize = 32;

/// The helper's first line.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Greeting {
    pub hello: u32,
    /// Names the credential file; chosen by the setup snippet, never checked against anything.
    pub slug: String,
    /// The credential from the file or from `--delegate-token`, if any.
    pub token: Option<String>,
    /// A delegate holzi started itself (spec 007/009); its credential never touches a file.
    #[serde(default)]
    pub delegate: bool,
}

impl std::fmt::Debug for Greeting {
    /// Never prints the credential.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Greeting")
            .field("hello", &self.hello)
            .field("slug", &self.slug)
            .field("token", &self.token.as_ref().map(|_| "…"))
            .field("delegate", &self.delegate)
            .finish()
    }
}

/// Why a greeting is refused before any reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HelloError {
    #[error("the greeting is too long")]
    TooLong,
    #[error("the greeting is not valid")]
    Malformed,
    #[error("the greeting has an unknown version")]
    Version,
    #[error("the slug is not valid")]
    Slug,
}

/// Whether `slug` matches `[a-z0-9-]{1,32}`.
pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= MAX_SLUG_CHARS
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Reads the helper's greeting line (without its newline).
pub fn parse_greeting(line: &[u8]) -> Result<Greeting, HelloError> {
    if line.len() > MAX_GREETING_BYTES {
        return Err(HelloError::TooLong);
    }
    let greeting: Greeting = serde_json::from_slice(line).map_err(|_| HelloError::Malformed)?;
    if greeting.hello != HELLO_VERSION {
        return Err(HelloError::Version);
    }
    if !valid_slug(&greeting.slug) {
        return Err(HelloError::Slug);
    }
    Ok(greeting)
}

/// Why the app refuses a greeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectReason {
    /// The user blocked this agent.
    Blocked,
    /// Too many registrations wait already (FR-011).
    RegistrationBusy,
    /// The access is off.
    Disabled,
    /// The greeting could not be read.
    Malformed,
    /// A delegate whose pass is unknown or ended, or that lost its registration (US6).
    NotRegistered,
}

/// The app's answer to a greeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HelloReply {
    /// The credential is valid.
    Accepted,
    /// The registration dialog is open; the credential comes later as a control line.
    Pending,
    Rejected {
        reason: RejectReason,
    },
}

/// Why the app ends a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    /// The user declined the registration.
    Denied,
    Revoked,
    Expired,
    VaultClosed,
    Disabled,
}

/// What a control line carries.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Control {
    /// The new credential after a registration.
    Token(String),
    Closing(CloseReason),
}

impl std::fmt::Debug for Control {
    /// Never prints the credential.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Token(_) => f.write_str("Token(…)"),
            Self::Closing(reason) => f.debug_tuple("Closing").field(reason).finish(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlLine {
    holzi_control: Control,
}

/// The line for `control`, with its newline.
pub fn control_line(control: Control) -> String {
    let mut line = serde_json::to_string(&ControlLine {
        holzi_control: control,
    })
    .unwrap_or_default();
    line.push('\n');
    line
}

/// The control in `line`, or `None` for anything else (an MCP message passes on unchanged).
pub fn read_control(line: &[u8]) -> Option<Control> {
    let value: Value = serde_json::from_slice(line).ok()?;
    let object = value.as_object()?;
    if object.len() != 1 || !object.contains_key("holzi_control") {
        return None;
    }
    serde_json::from_value::<ControlLine>(value)
        .ok()
        .map(|line| line.holzi_control)
}

/// A line with its newline, for greeting and reply.
pub fn json_line(value: &impl Serialize) -> String {
    let mut line = serde_json::to_string(value).unwrap_or_default();
    line.push('\n');
    line
}
