//! Import and removal of custom sets (spec 053 US3, contracts/tauri-commands.md): the commands
//! after the pattern of `extension_install_preview` and `extension_install`. The window gives a
//! picked file or a folder, never the bytes; both steps read the source, the second read decides.

use std::path::Path;

use haex_crdt::CrdtTransaction;
use serde::Deserialize;
use tauri::{AppHandle, State};
use ts_rs::TS;

use super::import::{iso8601, prepare, IconImportReport, ImportError, Prepared, MAX_SETS};
use super::sources::{Archive, Files, Folder, MAX_ARCHIVE_BYTES};
use super::store::{self, NewSet};
use super::{IconSetInfo, IconSetRef, DEFAULT_SET, SELECTION_KEY};
use crate::error::{HolziError, Result};
use crate::files::{picked, PickedFile};
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::preferences::{self, PrefScope};
use crate::vault_gate::VaultDb;

/// Where a set comes from: a file chosen in the dialog (an archive or a VSIX) or, where the device
/// can read folders, a folder.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum IconSource {
    File { file: PickedFile },
    Folder { path: String },
}

impl From<ImportError> for HolziError {
    fn from(error: ImportError) -> Self {
        HolziError::InvalidInput {
            reason: error.to_string(),
        }
    }
}

fn join_error(error: tauri::Error) -> HolziError {
    HolziError::InvalidInput {
        reason: format!("import task: {error}"),
    }
}

/// Opens the source and its name (the file name without extension, or the folder name).
fn open(opener: &AppHandle, source: &IconSource) -> Result<(Box<dyn Files>, String)> {
    match source {
        IconSource::File { file } => {
            let bytes = picked::read(opener, file, MAX_ARCHIVE_BYTES as u64)?;
            let name = picked::display_name(opener, file);
            let stem = name
                .rsplit_once('.')
                .map_or(name.as_str(), |(stem, _)| stem);
            Ok((Box::new(Archive::open(bytes)?), stem.to_owned()))
        }
        IconSource::Folder { path } => {
            if !crate::platform::capabilities().folder_pick {
                return Err(ImportError::FolderUnavailable.into());
            }
            let folder = Path::new(path);
            let name = folder
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok((Box::new(Folder::open(folder)?), name))
        }
    }
}

/// What the import would write, with the name clash of the vault; writes nothing.
pub fn preview(db: &VaultDb, files: &dyn Files, fallback: &str) -> Result<IconImportReport> {
    let Prepared { mut report, .. } = prepare(files, fallback)?;
    let name = report.name.clone();
    report.same_name = db.read_blocking(move |q| store::id_by_name(q, &name))?;
    Ok(report)
}

/// Writes the set as one row inside the transaction `tx`. With `replace` the set of that id goes
/// in the same transaction, and the choice that pointed at it points at the new one. A refusal
/// writes nothing and comes back as the inner error.
pub fn write_set(
    tx: &mut CrdtTransaction<'_>,
    prepared: &Prepared,
    id: &str,
    created_at: &str,
    replace: Option<&str>,
) -> haex_crdt::Result<std::result::Result<(), ImportError>> {
    let held = store::count(tx)?;
    if let Some(old) = replace {
        if !store::exists(tx, old)? {
            return Ok(Err(ImportError::NoSuchSet));
        }
    } else if held >= MAX_SETS {
        return Ok(Err(ImportError::TooManySets));
    }
    store::insert(
        tx,
        id,
        &NewSet {
            name: &prepared.report.name,
            author: &prepared.report.author,
            license: &prepared.report.license,
            source: &prepared.source,
            image_count: prepared.report.image_count,
            sha256: &prepared.sha256,
            pack: &prepared.pack,
            created_at,
        },
    )?;
    if let Some(old) = replace {
        let old_ref = IconSetRef::Custom { id: old.to_owned() };
        if store::selection_value(tx)?.as_deref() == Some(old_ref.selection_value().as_str()) {
            let new_ref = IconSetRef::Custom { id: id.to_owned() };
            preferences::insert_or_update(
                tx,
                PrefScope::Vault,
                SELECTION_KEY,
                &new_ref.selection_value(),
            )?;
        }
        store::delete(tx, old)?;
    }
    Ok(Ok(()))
}

/// Reads the source and writes the set (see [`write_set`]); returns it as the list shows it.
pub fn import(
    db: &VaultDb,
    files: &dyn Files,
    fallback: &str,
    replace: Option<String>,
    now_secs: i64,
) -> Result<IconSetInfo> {
    let prepared = prepare(files, fallback)?;
    let id = uuid::Uuid::new_v4().to_string();
    let created_at = iso8601(now_secs);
    db.write_blocking(|tx| write_set(tx, &prepared, &id, &created_at, replace.as_deref()))??;
    let sets = db.read_blocking(|q| super::commands::list(q))?;
    sets.into_iter()
        .find(|info| info.set_ref == IconSetRef::Custom { id: id.clone() })
        .ok_or_else(|| HolziError::InvalidInput {
            reason: "the imported set is not in the list".to_owned(),
        })
}

/// Whether `id` names a custom set: a bundled set has no id of this shape and cannot be removed.
pub fn check_set_id(id: &str) -> Result<()> {
    if super::is_set_id(id) {
        Ok(())
    } else {
        Err(HolziError::InvalidInput {
            reason: "only an own icon set can be removed".to_owned(),
        })
    }
}

/// Deletes a custom set inside the transaction `tx`; when the vault had chosen it, the choice
/// falls back to the default set in the same transaction. A set that is not there writes nothing
/// and comes back as the inner error.
pub fn remove_set(
    tx: &mut CrdtTransaction<'_>,
    id: &str,
) -> haex_crdt::Result<std::result::Result<(), ImportError>> {
    if !store::exists(tx, id)? {
        return Ok(Err(ImportError::NoSuchSet));
    }
    let set = IconSetRef::Custom { id: id.to_owned() };
    if store::selection_value(tx)?.as_deref() == Some(set.selection_value().as_str()) {
        preferences::insert_or_update(
            tx,
            PrefScope::Vault,
            SELECTION_KEY,
            &DEFAULT_SET.selection_value(),
        )?;
    }
    store::delete(tx, id)?;
    Ok(Ok(()))
}

/// Reads the source and tells what an import would do; writes nothing.
#[tauri::command]
pub async fn file_icon_set_import_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    source: IconSource,
) -> Result<IconImportReport> {
    let db = active_database(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (files, name) = open(&app, &source)?;
        preview(&db, files.as_ref(), &name)
    })
    .await
    .map_err(join_error)?
}

/// Reads the source again and writes the set; `replace` is the id of the set it replaces.
#[tauri::command]
pub async fn file_icon_set_import(
    app: AppHandle,
    state: State<'_, AppState>,
    source: IconSource,
    replace: Option<String>,
) -> Result<IconSetInfo> {
    let db = active_database(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (files, name) = open(&app, &source)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs() as i64);
        import(&db, files.as_ref(), &name, replace, now)
    })
    .await
    .map_err(join_error)?
}

/// Removes a custom set of the vault.
#[tauri::command]
pub async fn file_icon_set_remove(state: State<'_, AppState>, id: String) -> Result<()> {
    check_set_id(&id)?;
    let db = active_database(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        db.write_blocking(|tx| remove_set(tx, &id))??;
        Ok(())
    })
    .await
    .map_err(join_error)?
}

#[cfg(test)]
#[path = "commands_import_tests.rs"]
mod tests;
