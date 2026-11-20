//! The dialogs of the filesystem API (spec 017, US9, FR-048, research R19 steps 4 and 5): what the
//! user chooses becomes a choice of the frame that asked, for exactly that file or folder, reading
//! when opening and reading and writing when saving, until the frame closes. `save_file` writes in
//! the same call; `open_file` and `show_image` hand a copy in haex-vault's scratch folder, named only
//! by its file name, to the system's viewer.
//!
//! The open and save dialogs hand over a chosen file ([`PickedFile`], spec 043 FR-022): a path on a
//! desktop, a `content://` address of a document provider on Android. The frame gets what the
//! system named; for an address it reads and writes exactly that document through the file plugin
//! and never learns a path.

use std::path::{Path, PathBuf};

use base64::Engine;
use serde_json::{json, Value};
use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::DialogExt;

use super::{resolve, touches_denied, Access, FsEnvironment, Reach};
use crate::extensions::bridge::dispatch::CallContext;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::files::picked::{self, PickedFile};

pub const MODULE: &str = module_path!();

/// What a file dialog asks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DialogRequest {
    pub title: Option<String>,
    /// A folder to start in, or a file name to suggest when saving.
    pub default_path: Option<String>,
    /// File type filters: a name and its extensions.
    pub filters: Vec<(String, Vec<String>)>,
    pub multiple: bool,
}

/// The system's file dialogs and viewer. haex-vault's own is [`TauriDialogs`]; tests use a fake.
pub trait FileDialogs: Send + Sync {
    fn save(&self, request: DialogRequest) -> Option<PickedFile>;
    fn pick_folder(&self, request: DialogRequest) -> Option<PathBuf>;
    fn pick_files(&self, request: DialogRequest) -> Option<Vec<PickedFile>>;
    /// Opens `path` with the system's default program.
    fn open(&self, path: &Path) -> Result<(), String>;
}

/// The dialogs of tauri-plugin-dialog and the viewer of tauri-plugin-opener.
pub struct TauriDialogs<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> TauriDialogs<R> {
    fn builder(&self, request: &DialogRequest) -> tauri_plugin_dialog::FileDialogBuilder<R> {
        let mut builder = self.0.dialog().file();
        if let Some(title) = &request.title {
            builder = builder.set_title(title);
        }
        for (name, extensions) in &request.filters {
            let extensions: Vec<&str> = extensions.iter().map(String::as_str).collect();
            builder = builder.add_filter(name, &extensions);
        }
        builder
    }
}

impl<R: Runtime> FileDialogs for TauriDialogs<R> {
    fn save(&self, request: DialogRequest) -> Option<PickedFile> {
        let mut builder = self.builder(&request);
        if let Some(default) = &request.default_path {
            let default = Path::new(default);
            match (default.parent(), default.file_name()) {
                (Some(dir), Some(name)) if default.is_absolute() => {
                    builder = builder
                        .set_directory(dir)
                        .set_file_name(name.to_string_lossy());
                }
                _ => builder = builder.set_file_name(default.to_string_lossy()),
            }
        }
        Some(PickedFile(builder.blocking_save_file()?.to_string()))
    }

    #[cfg(desktop)]
    fn pick_folder(&self, request: DialogRequest) -> Option<PathBuf> {
        let mut builder = self.builder(&request);
        if let Some(dir) = &request.default_path {
            builder = builder.set_directory(dir);
        }
        builder.blocking_pick_folder()?.into_path().ok()
    }

    /// Mobile systems have no folder picker (spec 043 FR-016); `select_folder` answers "not
    /// available" before it gets here.
    #[cfg(mobile)]
    fn pick_folder(&self, _request: DialogRequest) -> Option<PathBuf> {
        None
    }

    fn pick_files(&self, request: DialogRequest) -> Option<Vec<PickedFile>> {
        let mut builder = self.builder(&request);
        if let Some(dir) = &request.default_path {
            builder = builder.set_directory(dir);
        }
        let chosen = if request.multiple {
            builder.blocking_pick_files()?
        } else {
            vec![builder.blocking_pick_file()?]
        };
        Some(
            chosen
                .into_iter()
                .map(|p| PickedFile(p.to_string()))
                .collect(),
        )
    }

    #[cfg(desktop)]
    fn open(&self, path: &Path) -> Result<(), String> {
        use tauri_plugin_opener::OpenerExt;
        self.0
            .opener()
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_err(|e| e.to_string())
    }

    #[cfg(mobile)]
    fn open(&self, _path: &Path) -> Result<(), String> {
        Err("no viewer on this platform".to_owned())
    }
}

fn invalid(message: &str) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Validation, message)
}

fn failed(message: impl Into<String>) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Filesystem, message)
}

fn text(params: &Value, name: &str) -> Option<String> {
    params.get(name).and_then(Value::as_str).map(str::to_owned)
}

/// The bytes of `data`, an array of byte values as the SDK sends them.
fn bytes(params: &Value, max: usize) -> Result<Vec<u8>, BridgeError> {
    let values = params
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("data must be an array of bytes"))?;
    if values.len() > max {
        return Err(BridgeError::new(
            ExtensionErrorCode::LimitExceeded,
            "data too large",
        ));
    }
    values
        .iter()
        .map(|v| {
            v.as_u64()
                .and_then(|b| u8::try_from(b).ok())
                .ok_or_else(|| invalid("data must be an array of bytes"))
        })
        .collect()
}

/// A chosen file as the system named it: a path, or a document of a provider that has none.
enum Chosen {
    Path(PathBuf),
    Document(PickedFile),
}

fn classify(environment: &FsEnvironment, file: PickedFile) -> Result<Chosen, BridgeError> {
    match picked::resolve(environment.opener.as_ref(), &file) {
        Ok(tauri_plugin_fs::FilePath::Path(path)) => Ok(Chosen::Path(path)),
        Ok(tauri_plugin_fs::FilePath::Url(_)) => Ok(Chosen::Document(file)),
        Err(error) => Err(invalid(&error.to_string())),
    }
}

/// The error of reading or writing a chosen document, for the frame.
pub(super) fn document_failed(error: crate::error::HaexError) -> BridgeError {
    failed(error.to_string())
}

/// The largest answer of the caller, which also bounds what it may hand over.
fn max_bytes(ctx: &CallContext) -> Result<usize, BridgeError> {
    let id = ctx.session.extension_id;
    let limits = ctx
        .db
        .read_blocking(move |q| crate::extensions::sql::exec::limits_of(q, id))
        .map_err(|_| BridgeError::new(ExtensionErrorCode::Database, "database unavailable"))?;
    Ok(usize::try_from(limits.max_response_bytes).unwrap_or(usize::MAX))
}

/// `{data, defaultPath?, title?, filters?: [{name, extensions}]}` → `{path, success}` or `null`
/// when the user cancels; the chosen file may then be read and written by this frame.
pub fn save_file(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let environment = ctx.host.fs.environment()?;
    let data = bytes(params, max_bytes(ctx)?)?;
    let filters = params
        .get("filters")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|f| {
                    let name = f.get("name")?.as_str()?.to_owned();
                    let extensions = f
                        .get("extensions")?
                        .as_array()?
                        .iter()
                        .filter_map(|e| e.as_str().map(str::to_owned))
                        .collect();
                    Some((name, extensions))
                })
                .collect()
        })
        .unwrap_or_default();
    let request = DialogRequest {
        title: text(params, "title"),
        default_path: text(params, "defaultPath"),
        filters,
        multiple: false,
    };
    let Some(chosen) = environment.dialogs.save(request) else {
        return Ok(Value::Null);
    };
    let chosen = match classify(&environment, chosen)? {
        Chosen::Path(path) => path,
        Chosen::Document(file) => {
            picked::write(environment.opener.as_ref(), &file, &data).map_err(document_failed)?;
            ctx.host
                .fs
                .grant_document(&ctx.session.frame, file.clone(), Access::Write);
            return Ok(json!({ "path": file.0, "success": true }));
        }
    };
    let path = resolve(&chosen.to_string_lossy())?;
    if touches_denied(&environment, &path, Reach::Path) {
        return Err(super::protected());
    }
    std::fs::write(&path, data).map_err(|e| failed(format!("write failed: {e}")))?;
    ctx.host
        .fs
        .grant(&ctx.session.frame, path.clone(), Access::Write, Reach::Path);
    Ok(json!({ "path": path.to_string_lossy(), "success": true }))
}

/// A fresh folder in haex-vault's scratch place and `name` reduced to a plain file name in it.
fn scratch_file(scratch: &Path, name: &str) -> Result<PathBuf, BridgeError> {
    let name = Path::new(name)
        .file_name()
        .ok_or_else(|| invalid("fileName must name a file"))?;
    let dir = scratch.join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&dir).map_err(|e| failed(format!("scratch folder: {e}")))?;
    Ok(dir.join(name))
}

/// Copies in the scratch place older than this are removed when haex-vault starts.
const SCRATCH_KEEP: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// Removes the copies an earlier start handed to the viewer, so files of the user do not pile up
/// in haex-vault's cache. Recent ones stay: another haex-vault process may have just opened them.
pub fn prune_scratch(scratch: &Path) {
    let Ok(entries) = std::fs::read_dir(scratch) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|time| time.elapsed().is_ok_and(|age| age > SCRATCH_KEEP));
        if old && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            if let Err(error) = std::fs::remove_dir_all(entry.path()) {
                log::warn!("extensions: an old opened copy stays: {error}");
            }
        }
    }
}

/// File types the system's viewer may open for an extension: documents, pictures and media.
/// Anything else could be a program or a script the system would run (`.exe`, `.bat`, `.desktop`,
/// `.command`, a document with macros); a page (`.html`, `.svg`) would run scripts with access to
/// local files.
const VIEWABLE: &[&str] = &[
    "pdf", "txt", "md", "csv", "tsv", "json", "ics", "vcf", "epub", "odt", "ods", "odp", "odg",
    "docx", "xlsx", "pptx", "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "heic",
    "avif", "mp3", "m4a", "ogg", "oga", "opus", "wav", "flac", "mp4", "m4v", "webm", "mkv", "mov",
];

/// Whether the system's viewer may open a file named `name`. Also refused: `:` (an alternate data
/// stream on Windows), control characters, and a trailing dot or space, which Windows drops.
fn viewable(name: &str) -> bool {
    if name.chars().any(|c| c == ':' || c.is_control()) || name.ends_with(['.', ' ']) {
        return false;
    }
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIEWABLE.contains(&e.to_ascii_lowercase().as_str()))
}

/// `{data, fileName, mimeType?}`: the system's viewer opens a copy (FR-046), only of a document,
/// picture or media file ([`VIEWABLE`]).
pub fn open_file(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let environment = ctx.host.fs.environment()?;
    let data = bytes(params, max_bytes(ctx)?)?;
    let name = text(params, "fileName").ok_or_else(|| invalid("fileName is missing"))?;
    if !viewable(&name) {
        return Err(invalid("this file type cannot be opened"));
    }
    let path = scratch_file(&environment.scratch, &name)?;
    std::fs::write(&path, data).map_err(|e| failed(format!("write failed: {e}")))?;
    let opened = environment.dialogs.open(&path);
    if let Err(error) = &opened {
        log::warn!("extensions: the viewer did not open a file: {error}");
    }
    Ok(json!({ "success": opened.is_ok() }))
}

/// The extension a `data:` URL of an image is stored with; other types are refused.
fn image_extension(mime: &str) -> Option<&'static str> {
    match mime {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/bmp" => Some("bmp"),
        _ => None,
    }
}

/// `{dataUrl}` of an image: the system's viewer shows it.
pub fn show_image(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let environment = ctx.host.fs.environment()?;
    let url = text(params, "dataUrl").ok_or_else(|| invalid("dataUrl is missing"))?;
    let (header, payload) = url
        .strip_prefix("data:")
        .and_then(|rest| rest.split_once(','))
        .ok_or_else(|| invalid("dataUrl must be a data: URL"))?;
    let mime = header
        .strip_suffix(";base64")
        .ok_or_else(|| invalid("dataUrl must be base64"))?;
    let extension = image_extension(mime).ok_or_else(|| invalid("not an image"))?;
    let data = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|_| invalid("dataUrl is not valid base64"))?;
    if data.len() > max_bytes(ctx)? {
        return Err(BridgeError::new(
            ExtensionErrorCode::LimitExceeded,
            "image too large",
        ));
    }
    let path = scratch_file(&environment.scratch, &format!("image.{extension}"))?;
    std::fs::write(&path, data).map_err(|e| failed(format!("write failed: {e}")))?;
    let opened = environment.dialogs.open(&path);
    Ok(json!({ "success": opened.is_ok() }))
}

/// `{title?, defaultPath?}` → the chosen folder or `null`; this frame may read in it.
pub fn select_folder(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    BridgeError::unless(crate::platform::capabilities().folder_pick)?;
    let environment = ctx.host.fs.environment()?;
    let request = DialogRequest {
        title: text(params, "title"),
        default_path: text(params, "defaultPath"),
        ..DialogRequest::default()
    };
    let Some(chosen) = environment.dialogs.pick_folder(request) else {
        return Ok(Value::Null);
    };
    let path = resolve(&chosen.to_string_lossy())?;
    if touches_denied(&environment, &path, Reach::Tree) {
        return Err(super::protected());
    }
    ctx.host
        .fs
        .grant(&ctx.session.frame, path.clone(), Access::Read, Reach::Tree);
    Ok(json!(path.to_string_lossy()))
}

/// `{title?, defaultPath?, filters?: [[name, [extensions]]], multiple?}` → the chosen files or
/// `null`; this frame may read them.
pub fn select_file(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let environment = ctx.host.fs.environment()?;
    let filters = params
        .get("filters")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|f| {
                    let pair = f.as_array()?;
                    let name = pair.first()?.as_str()?.to_owned();
                    let extensions = pair
                        .get(1)?
                        .as_array()?
                        .iter()
                        .filter_map(|e| e.as_str().map(str::to_owned))
                        .collect();
                    Some((name, extensions))
                })
                .collect()
        })
        .unwrap_or_default();
    let request = DialogRequest {
        title: text(params, "title"),
        default_path: text(params, "defaultPath"),
        filters,
        multiple: params.get("multiple").and_then(Value::as_bool) == Some(true),
    };
    let Some(chosen) = environment.dialogs.pick_files(request) else {
        return Ok(Value::Null);
    };
    let mut paths = Vec::with_capacity(chosen.len());
    for file in chosen {
        let file = match classify(&environment, file)? {
            Chosen::Path(path) => path,
            Chosen::Document(file) => {
                ctx.host
                    .fs
                    .grant_document(&ctx.session.frame, file.clone(), Access::Read);
                paths.push(file.0);
                continue;
            }
        };
        let path = resolve(&file.to_string_lossy())?;
        if touches_denied(&environment, &path, Reach::Path) {
            return Err(super::protected());
        }
        ctx.host
            .fs
            .grant(&ctx.session.frame, path.clone(), Access::Read, Reach::Path);
        paths.push(path.to_string_lossy().into_owned());
    }
    Ok(json!(paths))
}

#[cfg(test)]
#[path = "dialogs_tests.rs"]
mod tests;
