//! Shells of extensions (spec 017, US11, T110, FR-057–FR-059, research R21): the SDK's
//! `client.shell` on a PTY (`portable-pty`, Unix PTYs and Windows ConPTY).
//!
//! Every program needs a `shell` permission for its canonical path (`execute`); the question in
//! haex-vault's window warns that the extension can then do anything the user can on this device. A
//! session belongs to the extension that started it: only it writes, resizes or closes it, and its
//! output and its end reach only that extension's frames (`shell:output`, `shell:exit`), read
//! only as fast as the frames acknowledge it ([`flow`]). A session
//! ends with the extension's last frame, when it is disabled or removed, and with the vault: the
//! whole session of the shell, registered with the vault's
//! [`ChildRegistry`](crate::vault_gate::ChildRegistry) ([`session`]). Mobile devices have no
//! shell (8001).

pub(super) mod flow;
mod program;
mod session;

use std::path::Path;

use serde_json::{json, Value};

use crate::extensions::bridge::dispatch::CallContext;
use crate::extensions::error::{Asks, BridgeError, ExtensionErrorCode};
use crate::extensions::permissions::store::candidates;
use crate::extensions::permissions::{
    evaluate, Action, Decision, PermissionKind, PermissionRequest, RequestTarget,
};
pub use program::resolve_program;
use program::{available, default_program, home, lexical_absolute};
use session::Session;
pub use session::{ShellState, Utf8Stream};

pub const MODULE: &str = module_path!();

pub const OUTPUT: &str = "shell:output";
pub const EXIT: &str = "shell:exit";

/// Bytes of one `write`.
pub const MAX_WRITE_BYTES: usize = 64 * 1024;
/// Columns and rows of a terminal.
pub const MAX_SIZE: u16 = 1000;
/// Environment variables of one session, and the bytes of each name and value.
pub const MAX_ENV: usize = 128;
pub const MAX_ENV_BYTES: usize = 4096;
/// Bytes of the program an extension names (`options.shell`).
pub const MAX_PROGRAM_BYTES: usize = 4096;
/// Sessions one extension may have open at a time.
pub const MAX_SESSIONS: usize = 16;
/// Output events one `ack` may acknowledge.
pub const MAX_ACK: u64 = 1_000_000;

fn invalid(message: &str) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Validation, message)
}

fn shell_error(message: impl Into<String>) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Shell, message)
}

fn not_found() -> BridgeError {
    BridgeError::new(ExtensionErrorCode::NotFound, "not found")
}

/// Whether the extension may run `program`, the path a permission names; `None` for a bare name
/// that is not on `PATH`, which no permission can name. Every answer tells the program as the
/// extension `named` it (`details.target`); the question asks about `program`, so the answer
/// tells neither where a link points nor whether a name is on `PATH`.
fn check_program(
    ctx: &CallContext,
    program: Option<&Path>,
    named: &str,
) -> Result<(), BridgeError> {
    let (extension_id, device) = (ctx.session.extension_id, ctx.device);
    let mut grants = ctx
        .db
        .read_blocking(move |q| {
            candidates(q, extension_id, PermissionKind::Shell, device).map_err(Into::into)
        })
        .map_err(|_| BridgeError::new(ExtensionErrorCode::Database, "database unavailable"))?;
    grants.extend(
        ctx.host
            .permissions
            .temporary(extension_id, PermissionKind::Shell),
    );
    let request = PermissionRequest {
        kind: PermissionKind::Shell,
        action: Action::Execute,
        target: RequestTarget::Program(program.unwrap_or(Path::new(named)).to_path_buf()),
    };
    let code = match evaluate(&grants, &request, device) {
        Decision::Allow => return Ok(()),
        Decision::Deny => ExtensionErrorCode::PermissionDenied,
        Decision::Prompt => ExtensionErrorCode::PermissionPromptRequired,
    };
    let asks = match program {
        Some(program) => Asks::Target(program.to_string_lossy().into_owned()),
        None => Asks::Nothing,
    };
    Err(BridgeError::new(code, "permission required")
        .with_details(json!({
            "resourceType": "shell",
            "action": "execute",
            "target": named,
        }))
        .asking(asks))
}

fn size(value: Option<&Value>, default: u16) -> Result<u16, BridgeError> {
    match value {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .filter(|n| (1..=MAX_SIZE).contains(n))
            .ok_or_else(|| invalid("cols and rows must be numbers from 1 to 1000")),
    }
}

fn environment(value: Option<&Value>) -> Result<Vec<(String, String)>, BridgeError> {
    let Some(map) = value.filter(|v| !v.is_null()) else {
        return Ok(Vec::new());
    };
    let map = map
        .as_object()
        .filter(|m| m.len() <= MAX_ENV)
        .ok_or_else(|| invalid("env must be an object of at most 128 strings"))?;
    map.iter()
        .map(|(name, value)| {
            let value = value
                .as_str()
                .ok_or_else(|| invalid("env values must be strings"))?;
            let fits = |s: &str| s.len() <= MAX_ENV_BYTES && !s.contains('\0');
            if name.is_empty() || name.contains('=') || !fits(name) || !fits(value) {
                return Err(invalid("env name or value not allowed"));
            }
            Ok((name.clone(), value.to_owned()))
        })
        .collect()
}

/// The terminal a shell runs on, as programs read it from `TERM`.
pub const TERM: &str = "xterm-256color";

/// Variables of haex-vault's own process that a shell does not inherit: haex-vault's own settings and those
/// of the wrappers that start it (the Nix devShell and `scripts/with-nix-host-bridge.sh`, Nix's
/// GTK wrappers, an AppImage), which point the dynamic loader, GTK, GIO, GStreamer, WebKit and the
/// graphics drivers at haex-vault's libraries. Everything else the user's session set stays (`HOME`,
/// `USER`, `PATH`, `LANG`, `LC_*`, `SHELL`, `DISPLAY`, ...); `PATH` too, even when a wrapper
/// extended it.
const NOT_INHERITED: &[&str] = &[
    "APPDIR",
    "APPIMAGE",
    "ARGV0",
    "CUDA_ROOT",
    "GBM_BACKENDS_PATH",
    "GSETTINGS_SCHEMA_DIR",
    "LIBCLANG_PATH",
    "OWD",
    "PKG_CONFIG_PATH",
    "RUST_BACKTRACE",
    "RUST_LIB_BACKTRACE",
    "RUST_LOG",
];
const NOT_INHERITED_PREFIXES: &[&str] = &[
    "DYLD_",
    "GDK_",
    "GIO_",
    "GST_PLUGIN_",
    "GTK_",
    "HAEX_",
    "LD_",
    "TAURI_",
    "WEBKIT_",
    "__EGL_",
    "__GLX_",
    "__NV_",
];

/// Whether a shell does not inherit haex-vault's variable `name` ([`NOT_INHERITED`]).
pub fn not_inherited(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    NOT_INHERITED.contains(&name.as_str())
        || NOT_INHERITED_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
}

/// `portable-pty` starts from haex-vault's whole environment; this takes haex-vault's own variables out
/// and sets `TERM`.
fn clean_environment(command: &mut portable_pty::CommandBuilder) {
    for (name, _) in std::env::vars_os() {
        if not_inherited(&name.to_string_lossy()) {
            command.env_remove(name);
        }
    }
    command.env("TERM", TERM);
}

/// The terminal exists only where the capability table says so (spec 043 FR-016).
fn desktop_only() -> Result<(), BridgeError> {
    BridgeError::unless(crate::platform::capabilities().terminal)
}

/// `extension_shell_list_available` → `[{name, path}]`; needs no permission.
pub fn list_available(_ctx: &CallContext, _params: &Value) -> Result<Value, BridgeError> {
    desktop_only()?;
    Ok(Value::Array(
        available()
            .into_iter()
            .map(|(name, path)| json!({ "name": name, "path": path.to_string_lossy() }))
            .collect(),
    ))
}

/// `extension_shell_create {options: {shell?, cwd?, cols?, rows?, env?}}` → `{sessionId, shellName}`.
pub fn create(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    desktop_only()?;
    let options = match params.get("options") {
        None | Some(Value::Null) => json!({}),
        Some(o) if o.is_object() => o.clone(),
        Some(_) => return Err(invalid("options must be an object")),
    };
    let named = match options.get("shell") {
        None | Some(Value::Null) => default_program(),
        Some(Value::String(s)) if s.len() > MAX_PROGRAM_BYTES => {
            return Err(invalid("shell too long"))
        }
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        Some(_) => return Err(invalid("shell must be a string")),
    };
    let program = resolve_program(&named);
    let cols = size(options.get("cols"), 80)?;
    let rows = size(options.get("rows"), 24)?;
    let env = environment(options.get("env"))?;
    let cwd = options.get("cwd").filter(|v| !v.is_null());
    if cwd.is_some_and(|dir| !dir.is_string()) {
        return Err(invalid("cwd must be a string"));
    }
    // The permission comes first: without it, an answer must not tell which files exist. A
    // program that is not there is asked about by the absolute path it was named by; a bare name
    // that is not on `PATH` cannot be granted, so nobody is asked, and the extension waits as for
    // a question nobody answers.
    let asked = program.clone().or_else(|| lexical_absolute(&named));
    check_program(ctx, asked.as_deref(), &named)?;
    let program = program.ok_or_else(|| shell_error("program not found"))?;
    if ctx.host.shells.count(ctx.session.extension_id) >= MAX_SESSIONS {
        return Err(BridgeError::new(
            ExtensionErrorCode::LimitExceeded,
            "too many shells",
        ));
    }
    // Looked at only with the permission: before it, an answer must not tell which folders exist.
    let cwd = match cwd.and_then(Value::as_str) {
        None => home(),
        Some(dir) => Some(
            std::fs::canonicalize(dir)
                .ok()
                .filter(|d| d.is_dir())
                .ok_or_else(|| shell_error("working directory not found"))?,
        ),
    };

    let mut command = portable_pty::CommandBuilder::new(&program);
    clean_environment(&mut command);
    if let Some(cwd) = cwd {
        command.cwd(cwd);
    }
    // The extension's own variables come last: with the permission it may set any of them.
    for (name, value) in env {
        command.env(name, value);
    }
    let size = portable_pty::PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    };
    let session_id = session::start(ctx, command, size)?;
    let shell_name = program
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    Ok(json!({ "sessionId": session_id, "shellName": shell_name }))
}

fn session_id(params: &Value) -> Result<String, BridgeError> {
    params
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| invalid("sessionId must be a string"))
}

/// Runs `f` on a session of the calling extension; another extension's is not found (FR-062).
fn with_own<T>(
    ctx: &CallContext,
    params: &Value,
    f: impl FnOnce(&Session) -> Result<T, BridgeError>,
) -> Result<T, BridgeError> {
    desktop_only()?;
    let id = session_id(params)?;
    let sessions = ctx.host.shells.lock();
    match sessions.get(&id) {
        Some(session) if session.extension_id == ctx.session.extension_id => f(session),
        _ => Err(not_found()),
    }
}

/// `extension_shell_write {sessionId, data}`.
pub fn write(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let data = params
        .get("data")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("data must be a string"))?;
    if data.len() > MAX_WRITE_BYTES {
        return Err(BridgeError::new(
            ExtensionErrorCode::LimitExceeded,
            "data too large",
        ));
    }
    with_own(ctx, params, |session| {
        session.write(data.as_bytes().to_vec())
    })?;
    Ok(Value::Null)
}

/// `extension_shell_resize {sessionId, cols, rows}`.
pub fn resize(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let cols = size(params.get("cols"), 0)?;
    let rows = size(params.get("rows"), 0)?;
    with_own(ctx, params, |session| {
        session.resize(portable_pty::PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
    })?;
    Ok(Value::Null)
}

/// `extension_shell_ack {sessionId, count}`: the calling frame handed on `count` more
/// `shell:output` events ([`flow`]).
pub fn ack(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let count = params
        .get("count")
        .and_then(Value::as_u64)
        .filter(|n| (1..=MAX_ACK).contains(n))
        .ok_or_else(|| invalid("count must be a number from 1 to 1000000"))?;
    with_own(ctx, params, |session| {
        session.acknowledge(&ctx.session.frame, count);
        Ok(())
    })?;
    Ok(Value::Null)
}

/// `extension_shell_close {sessionId}`: ends the process group; `shell:exit` follows.
pub fn close(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    desktop_only()?;
    let id = session_id(params)?;
    let session = {
        let mut sessions = ctx.host.shells.lock();
        match sessions.get(&id) {
            Some(s) if s.extension_id == ctx.session.extension_id => sessions.remove(&id),
            _ => None,
        }
    };
    session.ok_or_else(not_found)?.end();
    Ok(Value::Null)
}

#[cfg(all(test, unix))]
mod shell_tests;
