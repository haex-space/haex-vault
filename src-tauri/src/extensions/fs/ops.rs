//! The file operations of the filesystem API (spec 017, US9, FR-046, FR-047): every path goes
//! through [`super::authorize`] at its real target before anything is read or written. Contents
//! travel as base64, as the SDK sends and expects them.

use std::io::Read;
use std::path::Path;
use std::time::UNIX_EPOCH;

use base64::Engine;
use serde_json::{json, Map, Value};

use super::dialogs::document_failed;
use super::{authorize, Access, Reach};
use crate::extensions::bridge::dispatch::CallContext;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::files::encrypted::format::is_folder_prefix;
use crate::files::picked;

pub const MODULE: &str = module_path!();

fn invalid(message: &str) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Validation, message)
}

fn failed(what: &str, error: std::io::Error) -> BridgeError {
    BridgeError::new(
        ExtensionErrorCode::Filesystem,
        format!("{what} failed: {error}"),
    )
}

fn path_param<'a>(params: &'a Value, name: &str) -> Result<&'a str, BridgeError> {
    params
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(&format!("{name} must be a path")))
}

/// Milliseconds since 1970 of a file time, if the platform has it.
fn millis(time: std::io::Result<std::time::SystemTime>) -> Option<u64> {
    let since = time.ok()?.duration_since(UNIX_EPOCH).ok()?;
    u64::try_from(since.as_millis()).ok()
}

fn max_bytes(ctx: &CallContext) -> Result<u64, BridgeError> {
    let id = ctx.session.extension_id;
    ctx.db
        .read_blocking(move |q| crate::extensions::sql::exec::limits_of(q, id))
        .map(|limits| limits.max_response_bytes)
        .map_err(|_| BridgeError::new(ExtensionErrorCode::Database, "database unavailable"))
}

fn too_large() -> BridgeError {
    BridgeError::new(ExtensionErrorCode::LimitExceeded, "file too large")
}

/// `{path}` → the contents as base64, up to the caller's answer size. Only a regular file: a
/// device or a pipe has no size and might never end. The read stops past the limit, so a file that
/// grows after the size check cannot get through either.
pub fn read_file(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let raw = path_param(params, "path")?;
    if let Some(file) = ctx.host.fs.document(&ctx.session.frame, raw, Access::Read) {
        // A chosen document without a path (spec 043 FR-022); the provider's size is not trusted.
        let opener = ctx.host.fs.environment()?.opener.clone();
        let max = max_bytes(ctx)?;
        let data = picked::read(opener.as_ref(), &file, max).map_err(document_failed)?;
        if u64::try_from(data.len()).unwrap_or(u64::MAX) > max {
            return Err(too_large());
        }
        return Ok(json!(base64::engine::general_purpose::STANDARD.encode(data)));
    }
    let path = authorize(ctx, raw, Access::Read, Reach::Path)?;
    let metadata = std::fs::metadata(&path).map_err(|e| failed("read", e))?;
    if !metadata.is_file() {
        return Err(invalid("path is not a file"));
    }
    let max = max_bytes(ctx)?;
    if metadata.len() > max {
        return Err(too_large());
    }
    let mut data = Vec::new();
    std::fs::File::open(&path)
        .and_then(|file| file.take(max.saturating_add(1)).read_to_end(&mut data))
        .map_err(|e| failed("read", e))?;
    if u64::try_from(data.len()).unwrap_or(u64::MAX) > max {
        return Err(too_large());
    }
    Ok(json!(base64::engine::general_purpose::STANDARD.encode(data)))
}

/// `{path, data}` with base64 contents.
pub fn write_file(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let raw = path_param(params, "path")?;
    let document = ctx.host.fs.document(&ctx.session.frame, raw, Access::Write);
    let path = match document {
        Some(_) => None,
        None => Some(authorize(ctx, raw, Access::Write, Reach::Path)?),
    };
    let data = params
        .get("data")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("data must be base64"))?;
    let data = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| invalid("data must be base64"))?;
    if let Some(file) = document {
        let opener = ctx.host.fs.environment()?.opener.clone();
        picked::write(opener.as_ref(), &file, &data).map_err(document_failed)?;
    } else if let Some(path) = path {
        std::fs::write(&path, data).map_err(|e| failed("write", e))?;
    }
    Ok(Value::Null)
}

/// `{path}` → the entries of a folder.
pub fn read_dir(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let path = authorize(ctx, path_param(params, "path")?, Access::Read, Reach::Path)?;
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&path).map_err(|e| failed("read_dir", e))? {
        let entry = entry.map_err(|e| failed("read_dir", e))?;
        let metadata = entry.metadata().map_err(|e| failed("read_dir", e))?;
        // The raw directory of an encrypted folder never shows (spec 048 FR-051).
        if metadata.is_dir() && entry.file_name().to_str().is_some_and(is_folder_prefix) {
            continue;
        }
        entries.push(json!({
            "name": entry.file_name().to_string_lossy(),
            "path": entry.path().to_string_lossy(),
            "isFile": metadata.is_file(),
            "isDirectory": metadata.is_dir(),
            "size": metadata.len(),
            "modified": millis(metadata.modified()),
        }));
    }
    Ok(Value::Array(entries))
}

/// `{path}`: creates the folder and the missing folders above it.
pub fn mkdir(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let path = authorize(ctx, path_param(params, "path")?, Access::Write, Reach::Path)?;
    std::fs::create_dir_all(&path).map_err(|e| failed("mkdir", e))?;
    Ok(Value::Null)
}

/// `{path, recursive?}`: removes a file, an empty folder, or with `recursive` a folder tree.
pub fn remove(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let recursive = params.get("recursive").and_then(Value::as_bool) == Some(true);
    let reach = if recursive { Reach::Tree } else { Reach::Path };
    let path = authorize(ctx, path_param(params, "path")?, Access::Write, reach)?;
    let metadata = std::fs::symlink_metadata(&path).map_err(|e| failed("remove", e))?;
    let removed = if !metadata.is_dir() {
        std::fs::remove_file(&path)
    } else if recursive {
        std::fs::remove_dir_all(&path)
    } else {
        std::fs::remove_dir(&path)
    };
    removed.map_err(|e| failed("remove", e))?;
    Ok(Value::Null)
}

/// `{path}` → whether it exists.
pub fn exists(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let raw = path_param(params, "path")?;
    if let Some(file) = ctx.host.fs.document(&ctx.session.frame, raw, Access::Read) {
        let opener = ctx.host.fs.environment()?.opener.clone();
        return Ok(json!(picked::open_read(opener.as_ref(), &file).is_ok()));
    }
    let path = authorize(ctx, raw, Access::Read, Reach::Path)?;
    Ok(json!(path.exists()))
}

/// `{path}` → size, kind, times and whether it is read-only.
pub fn stat(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let raw = path_param(params, "path")?;
    let path = authorize(ctx, raw, Access::Read, Reach::Path)?;
    let metadata = std::fs::metadata(&path).map_err(|e| failed("stat", e))?;
    let is_symlink = std::fs::symlink_metadata(raw).is_ok_and(|m| m.file_type().is_symlink());
    Ok(json!({
        "size": metadata.len(),
        "isFile": metadata.is_file(),
        "isDirectory": metadata.is_dir(),
        "isSymlink": is_symlink,
        "modified": millis(metadata.modified()),
        "created": millis(metadata.created()),
        "readonly": metadata.permissions().readonly(),
    }))
}

/// `{from, to}`: both ends need writing, and a folder takes its whole tree along.
pub fn rename(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let from = authorize(ctx, path_param(params, "from")?, Access::Write, Reach::Tree)?;
    let to = authorize(ctx, path_param(params, "to")?, Access::Write, Reach::Tree)?;
    std::fs::rename(&from, &to).map_err(|e| failed("rename", e))?;
    Ok(Value::Null)
}

/// Refuses to write at `target` below the copy's checked destination when it is already a link:
/// copying through it would write where nobody checked, perhaps into one of holzi's own places.
fn not_a_link(target: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(target) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(std::io::Error::other(format!(
            "{} is a link",
            target.display()
        ))),
        _ => Ok(()),
    }
}

/// Copies the tree at `from` (already checked at its real target) to `to`. A symbolic link inside
/// the tree is left out: its target was never checked and may be one of holzi's own places. For
/// the same reason nothing is written through a link that already lies below `to`.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    if !std::fs::metadata(from)?.is_dir() {
        std::fs::copy(from, to)?;
        return Ok(());
    }
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            continue;
        }
        let target = to.join(entry.file_name());
        not_a_link(&target)?;
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// `{from, to}`: reads the tree at `from`, writes at `to`.
pub fn copy(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let from = authorize(ctx, path_param(params, "from")?, Access::Read, Reach::Tree)?;
    let to = authorize(ctx, path_param(params, "to")?, Access::Write, Reach::Tree)?;
    if to.starts_with(&from) {
        return Err(invalid("cannot copy a folder into itself"));
    }
    copy_tree(&from, &to).map_err(|e| failed("copy", e))?;
    Ok(Value::Null)
}

/// The known places of this device that exist (`home`, `documents`, …); reading them still needs
/// a permission.
pub fn known_paths(ctx: &CallContext, _params: &Value) -> Result<Value, BridgeError> {
    let environment = ctx.host.fs.environment()?;
    let mut places = Map::new();
    for (name, path) in &environment.known {
        places.insert((*name).to_owned(), json!(path.to_string_lossy()));
    }
    Ok(Value::Object(places))
}
