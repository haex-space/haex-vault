//! Import methods of the service (spec 034, US7, FR-023): preview, run and the picture of an
//! imported icon. For the user alone (rule Z11). The file is read and parsed on blocking threads
//! (a KeePass key derivation can take seconds); the parse is a pure function of the bytes.

use std::io::Read;
use std::sync::atomic::AtomicBool;

use haex_crdt::rusqlite::params;
use zeroize::Zeroizing;

use super::{require_user, PasswordsService};
use crate::error::{HolziError, Result};
use crate::files::picked::{self, Opener, PickedFile};
use crate::passwords::access::Caller;
use crate::passwords::import::apply::{self, Control, OnDuplicate, Progress};
use crate::passwords::import::legacy_haex_vault::{
    self, WARNING_CLOSE_FIRST, WARNING_HAEX_PASS_TABLES,
};
use crate::passwords::import::{self, Credentials, ExistingKeys, ImportModel, ImportSource};
use crate::passwords::model::{ImportPreview, ImportReport};
use crate::passwords::{IMPORT_INPUT_LIMIT_BYTES, TRASH_GROUP_ID};
use crate::storage::query::Query;

fn failed(reason: &str) -> HolziError {
    HolziError::PasswordsImportFailed {
        reason: reason.to_string(),
    }
}

/// What the window hands over for a run: the format, the chosen file, and a KeePass password and
/// key file. The secrets are zeroed when dropped.
pub struct ImportRequest {
    pub source: ImportSource,
    pub file: PickedFile,
    pub password: Option<Zeroizing<String>>,
    pub key_file: Option<PickedFile>,
}

/// A file read: its model and what the preview says about the file besides the counts.
struct ReadFile {
    model: ImportModel,
    warnings: Vec<String>,
}

/// Reads all of a chosen file; an unreadable one is `unreadable`.
fn read_all(opener: &impl Opener, file: &PickedFile) -> Result<Zeroizing<Vec<u8>>> {
    let reader = picked::open_read(opener, file).map_err(|_| failed("unreadable"))?;
    read_all_limited(reader, IMPORT_INPUT_LIMIT_BYTES)
}

/// Reads a KeePass key file without applying the export-file limit. KeePass accepts XML, raw and
/// hexadecimal key files, and hashes arbitrary other contents; the parser needs the original
/// bytes to distinguish those formats.
fn read_key_file(opener: &impl Opener, file: &PickedFile) -> Result<Zeroizing<Vec<u8>>> {
    let mut bytes = Zeroizing::new(Vec::new());
    picked::open_read(opener, file)
        .map_err(|_| failed("unreadable"))?
        .read_to_end(&mut bytes)
        .map_err(|_| failed("unreadable"))?;
    Ok(bytes)
}

/// Reads at most one byte beyond the import limit, so provider streams cannot exhaust memory.
fn read_all_limited(reader: impl Read, limit: u64) -> Result<Zeroizing<Vec<u8>>> {
    let mut bytes = Zeroizing::new(Vec::new());
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| failed("unreadable"))?;
    if bytes.len() as u64 > limit {
        return Err(failed("too_large"));
    }
    Ok(bytes)
}

/// Reads the file (and key file) and parses it, off the async threads. A haex-vault 2.x (Legacy) file is a
/// database and is read from a path (spec 037): a chosen path as it is, together with its `-wal`;
/// a provider address (Android, spec 043) is copied into the app's temporary folder first, and a
/// `-wal` beside it cannot be reached (contract `picked-file.md`).
async fn read_model(
    opener: impl Opener + Send + 'static,
    request: ImportRequest,
) -> Result<ReadFile> {
    tauri::async_runtime::spawn_blocking(move || {
        if request.source == ImportSource::LegacyHaexVault {
            let credentials = Credentials {
                password: request.password,
                key_file: None,
            };
            let read = match picked::resolve(&opener, &request.file)? {
                tauri_plugin_fs::FilePath::Path(path) => {
                    legacy_haex_vault::read(&path, &credentials)?
                }
                tauri_plugin_fs::FilePath::Url(_) => {
                    let dir = tempfile::tempdir().map_err(|_| failed("unreadable"))?;
                    let copy = dir.path().join("legacy-haex-vault.db");
                    picked::copy_into(&opener, &request.file, &copy)
                        .map_err(|_| failed("unreadable"))?;
                    legacy_haex_vault::read(&copy, &credentials)?
                }
            };
            let mut warnings = vec![WARNING_CLOSE_FIRST.to_string()];
            if read.has_haex_pass_tables {
                warnings.push(WARNING_HAEX_PASS_TABLES.to_string());
            }
            return Ok(ReadFile {
                model: read.model,
                warnings,
            });
        }
        let bytes = read_all(&opener, &request.file)?;
        let key_file = match &request.key_file {
            Some(file) => Some(read_key_file(&opener, file)?),
            None => None,
        };
        let credentials = Credentials {
            password: request.password,
            key_file,
        };
        Ok(ReadFile {
            model: import::parse(request.source, &bytes, &credentials)?,
            warnings: Vec::new(),
        })
    })
    .await
    .map_err(|_| failed("unreadable"))?
}

impl PasswordsService {
    /// The (title, user name, address) of every entry outside the trash, for duplicate detection.
    async fn existing_keys(&self) -> Result<ExistingKeys> {
        self.db()
            .read(|q| {
                let rows: Vec<(Option<String>, Option<String>, Option<String>)> = q.query_map(
                    "WITH RECURSIVE trashed(id) AS ( \
                         SELECT ?1 \
                         UNION ALL \
                         SELECT g.id FROM haex_passwords_groups g \
                         JOIN trashed t ON g.parent_id = t.id) \
                     SELECT d.title, d.username, d.url FROM haex_passwords_item_details d \
                     WHERE d.id NOT IN (SELECT item_id FROM haex_passwords_group_items \
                                        WHERE group_id IN (SELECT id FROM trashed))",
                    params![TRASH_GROUP_ID],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )?;
                Ok(rows
                    .into_iter()
                    .map(|(t, u, l)| {
                        import::duplicate_key(t.as_deref(), u.as_deref(), l.as_deref())
                    })
                    .collect())
            })
            .await
    }

    /// Counts what the file would bring over; writes nothing.
    pub async fn import_preview(
        &self,
        caller: &Caller,
        opener: impl Opener + Send + 'static,
        request: ImportRequest,
    ) -> Result<ImportPreview> {
        require_user(caller)?;
        let read = read_model(opener, request).await?;
        let existing = self.existing_keys().await?;
        let mut preview = import::preview(&read.model, &existing);
        preview.warnings.splice(0..0, read.warnings);
        Ok(preview)
    }

    /// Writes the file in steps; see [`apply::run`]. `cancel` is the flag of the run's slot.
    pub async fn import_run(
        &self,
        caller: &Caller,
        opener: impl Opener + Send + 'static,
        request: ImportRequest,
        on_duplicate: OnDuplicate,
        cancel: &AtomicBool,
        progress: &(dyn Fn(Progress) + Send + Sync),
    ) -> Result<ImportReport> {
        require_user(caller)?;
        let model = read_model(opener, request).await?.model;
        let existing = self.existing_keys().await?;
        let control = Control {
            cancel,
            progress,
            inject: None,
        };
        apply::run(self.db(), model, &existing, on_duplicate, &control).await
    }

    /// The bytes of an imported picture (`icon = 'binary:<hash>'`).
    pub async fn icon_preview(&self, caller: &Caller, hash: String) -> Result<Vec<u8>> {
        require_user(caller)?;
        self.db()
            .read(move |q| {
                q.query_row(
                    "SELECT data FROM haex_passwords_binaries WHERE hash = ?1 AND type = 'icon'",
                    params![hash],
                    |r| r.get::<_, Vec<u8>>(0),
                )?
                .ok_or(HolziError::PasswordsNotFound)
                .map_err(Into::into)
            })
            .await
    }
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;
