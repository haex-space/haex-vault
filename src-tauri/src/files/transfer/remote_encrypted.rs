//! The encrypted halves of a transfer across sources (spec 048 US1, US5, T027, T044): into an
//! encrypted folder the bytes are sealed on the way (content object first, then the sidecar, then
//! the replaced version goes; nothing shows before its sidecar), out of one they are opened and
//! checked against the sidecar's SHA-256 before the part file becomes the file or the upload
//! completes. Between encrypted folders of one bucket the provider copies the content object under
//! a new `oid` and the new sidecar wraps its `DEK` again (format "Kopieren beim Anbieter"). Taken
//! names are asked about with the same hooks as everywhere (by their decrypted names).

use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

use tokio::io::AsyncReadExt;
use zeroize::Zeroizing;

use super::upload::Upload;
use super::{soon, Place, Run, Side, Stop, PART};
use crate::files::encrypted::format::sidecar::{FileFacts, Sidecar, SidecarKind};
use crate::files::encrypted::format::Id;
use crate::files::encrypted::manage::drop_replaced;
use crate::files::encrypted::ops::{now_ms, write_version};
use crate::files::encrypted::route::view;
use crate::files::encrypted::source::EncryptedFile;
use crate::files::encrypted::transfer::{ensure_folder, Decrypting, Sealed, SealedParts};
use crate::files::encrypted::tree::{facts, Item as TreeItem, Version};
use crate::files::encrypted::{damaged, new_id, OpenFolder};
use crate::files::kind::mime_for;
use crate::files::local::ops::io_error;
use crate::files::storage_source::{storage_error, StorageFiles};
use crate::files::transfer::local::CHUNK;
use crate::files::transfer::remote_plan::{EncSource, EncTarget, Item};
use crate::files::{FilesError, FilesErrorCode};
use crate::remote_storage::StorageError;

/// Reads plaintext for a sealed upload.
type Reader<'r> = dyn FnMut(&mut [u8]) -> Result<usize, FilesError> + 'r;

/// A content object complete at the target, for its sidecar.
struct NewFile {
    oid: Id,
    cid: Id,
    dek: Zeroizing<[u8; 32]>,
    size: u64,
    sha256: [u8; 32],
    modified: u64,
    content_type: Option<String>,
}

impl<'h> Run<'h, '_> {
    pub(super) fn enc_target(&self) -> &'h EncTarget {
        self.target
            .expect("an encrypted place has an encrypted target")
    }

    /// The target folder's names and then `rel`.
    pub(super) fn enc_path(&self, rel: &[String]) -> Vec<String> {
        let mut path = self.enc_target().rest.clone();
        path.extend_from_slice(rel);
        path
    }

    /// What lies at `rel` below the target: a file version, a folder, or nothing.
    pub(super) fn enc_lookup(&self, rel: &[String]) -> Result<Option<Option<Version>>, Stop> {
        let target = self.enc_target();
        let path = self.enc_path(rel);
        let view = self.handle.block_on(view(&target.files, &target.folder))?;
        Ok(view
            .tree
            .find(&path)
            .map(|index| match &view.tree.node(index).item {
                TreeItem::Version { version, .. } if facts(version).is_some() => {
                    Some((**version).clone())
                }
                _ => None,
            }))
    }

    /// The file a new one at `rel` replaces (the user chose to replace it); a folder there is
    /// `exists`.
    pub(super) fn replaced_at(&self, rel: &[String]) -> Result<Option<Version>, Stop> {
        match self.enc_lookup(rel)? {
            Some(None) => Err(Stop::Failed(FilesError::new(
                FilesErrorCode::Exists,
                "a folder has this name",
            ))),
            found => Ok(found.flatten()),
        }
    }

    pub(super) fn encrypted_taken(&self, rel: &[String]) -> Result<bool, Stop> {
        Ok(self.enc_lookup(rel)?.is_some())
    }

    /// The first free `<stem> (n)<ext>` beside `rel`.
    pub(super) fn encrypted_free(&self, mut rel: Vec<String>) -> Result<Vec<String>, Stop> {
        let name = rel.pop().unwrap_or_default();
        let (stem, ext) = match name.rfind('.') {
            Some(dot) if dot > 0 => name.split_at(dot),
            _ => (name.as_str(), ""),
        };
        for n in 2.. {
            let mut candidate = rel.clone();
            candidate.push(format!("{stem} ({n}){ext}"));
            if !self.encrypted_taken(&candidate)? {
                return Ok(candidate);
            }
        }
        unreachable!("some number is free")
    }

    /// Makes the folder `rel` below the target (one sidecar per missing folder); returns its entry
    /// id.
    pub(super) fn encrypted_folder(&self, rel: &[String]) -> Result<Id, Stop> {
        let target = self.enc_target();
        Ok(self.handle.block_on(ensure_folder(
            &target.files,
            &target.folder,
            &target.rest,
            rel,
        ))?)
    }

    /// Sends a file when one end is an encrypted folder.
    pub(super) fn send_encrypted(&mut self, item: &Item, place: &Place) -> Result<(), Stop> {
        match (&item.encrypted, place) {
            (Some(source), Place::Device(path)) => {
                let reading = self.open_source(source)?;
                self.download(path, move |run, part| run.decrypt_into(reading, part))
            }
            (Some(source), Place::Encrypted(rel)) if self.same_bucket() => {
                self.provider_copy(source, rel)
            }
            (Some(source), Place::Encrypted(rel)) => {
                let mut reading = self.open_source(source)?;
                let file = source.file().and_then(facts);
                let (modified, content_type, expected) = file
                    .map(|f| (f.modified, f.content_type.clone(), Some(f.sha256)))
                    .unwrap_or((now_ms(), None, None));
                self.seal_upload(rel, modified, content_type, expected, &mut |buf| {
                    reading.read(buf)
                })
            }
            (Some(source), Place::Key(key)) => self.decrypt_upload(source, key),
            (None, Place::Encrypted(rel)) => self.plain_into_encrypted(item, rel),
            (None, _) => Err(Stop::Failed(FilesError::new(
                FilesErrorCode::Unsupported,
                "not an encrypted transfer",
            ))),
        }
    }

    /// Whether source and target are one bucket, so the provider copies.
    fn same_bucket(&self) -> bool {
        match (&self.job.from, &self.job.to) {
            (Side::Storage(from), Side::Storage(to)) => {
                from.access().location == to.access().location
            }
            _ => false,
        }
    }

    /// Opens the file of `source` for reading through.
    fn open_source(&self, source: &EncSource) -> Result<Decrypting, Stop> {
        let files = self.source_files()?;
        let version = source
            .file()
            .ok_or_else(|| FilesError::invalid_path("not a file"))?;
        let expected = facts(version).map(|f| f.sha256).unwrap_or_default();
        let file = self.handle.block_on(EncryptedFile::open(
            &files,
            Arc::clone(&source.folder),
            version,
        ))?;
        Ok(Decrypting::new(file, expected, self.handle.clone()))
    }

    /// The storage the items come from.
    pub(super) fn source_files(&self) -> Result<Arc<StorageFiles>, Stop> {
        match &self.job.from {
            Side::Storage(files) => Ok(Arc::clone(files)),
            Side::Device => Err(Stop::Failed(FilesError::invalid_path("not from a storage"))),
        }
    }

    /// A file of an encrypted folder into an ordinary folder of a storage (the window said yes,
    /// FR-019): decrypted on the way; the upload completes only when the whole file checked out.
    fn decrypt_upload(&mut self, source: &EncSource, key: &str) -> Result<(), Stop> {
        let mut reading = self.open_source(source)?;
        let mut failure = None;
        let sent = self.upload(key, &mut |buf| {
            reading.read(buf).map_err(|error| {
                failure = Some(error);
                std::io::Error::other("the encrypted file could not be read")
            })
        });
        match (sent, failure) {
            (Err(_), Some(error)) => Err(Stop::Failed(error)),
            (sent, _) => sent,
        }
    }

    /// A file into an encrypted folder of the same bucket: the provider copies its content object
    /// under a new `oid`, and the new sidecar wraps the same `DEK` for the new entry (FR-018,
    /// FR-020).
    fn provider_copy(&mut self, source: &EncSource, rel: &[String]) -> Result<(), Stop> {
        let from = self.source_files()?;
        let version = source
            .file()
            .ok_or_else(|| FilesError::invalid_path("not a file"))?;
        let facts = facts(version).ok_or_else(damaged)?;
        let dek = source.folder.unwrap_dek(&version.sidecar.entry, facts)?;
        let target = self.enc_target();
        let (files, folder) = (Arc::clone(&target.files), Arc::clone(&target.folder));
        let (dirs, name) = rel.split_at(rel.len().saturating_sub(1));
        let parent = self.encrypted_folder(dirs)?;
        let replaced = self.replaced_at(rel)?;
        let oid = new_id();
        let (copied, original) = (
            folder.content_key(&oid),
            source.folder.content_key(&facts.content),
        );
        self.retried(|| {
            self.handle
                .block_on(from.store().copy(from.access(), &original, &copied, soon()))
        })?;
        self.done.bytes += facts.size;
        self.report();
        let file = NewFile {
            oid,
            cid: facts.cid,
            dek,
            size: facts.size,
            sha256: facts.sha256,
            modified: facts.modified,
            content_type: facts.content_type.clone(),
        };
        let name = name.first().cloned().unwrap_or_default();
        self.place_file(&files, &folder, parent, name, file, replaced)
    }

    /// Writes the plaintext of `reading` into `part`, then checks it whole.
    fn decrypt_into(&mut self, mut reading: Decrypting, part: &Path) -> Result<(), Stop> {
        let mut out = std::fs::File::create(part).map_err(|error| io_error(error, part))?;
        let mut buf = vec![0u8; CHUNK];
        loop {
            self.check()?;
            let read = reading.read(&mut buf)?;
            if read == 0 {
                break;
            }
            out.write_all(&buf[..read])
                .map_err(|error| io_error(error, part))?;
            self.done.bytes += read as u64;
            self.report();
        }
        reading.finish()?;
        out.sync_all().map_err(|error| io_error(error, part).into())
    }

    /// A file of the device or of an ordinary folder of a storage into the encrypted folder.
    fn plain_into_encrypted(&mut self, item: &Item, rel: &[String]) -> Result<(), Stop> {
        let name = rel.last().cloned().unwrap_or_default();
        let content_type = mime_for(&name).map(str::to_owned);
        match &self.job.from {
            Side::Device => {
                let path = Path::new(&item.from);
                let mut file = std::fs::File::open(path).map_err(|error| io_error(error, path))?;
                let modified = file
                    .metadata()
                    .ok()
                    .and_then(|meta| meta.modified().ok())
                    .map(crate::passwords::clock::unix_millis)
                    .and_then(|ms| u64::try_from(ms).ok())
                    .unwrap_or_else(now_ms);
                self.seal_upload(rel, modified, content_type, None, &mut |buf| {
                    file.read(buf).map_err(|error| io_error(error, path))
                })
            }
            Side::Storage(from) => {
                let from = Arc::clone(from);
                let mut reader = self.retried(|| {
                    self.handle.block_on(from.store().get_range(
                        from.access(),
                        &item.from,
                        0,
                        item.size,
                        soon(),
                    ))
                })?;
                let handle = self.handle.clone();
                self.seal_upload(rel, now_ms(), content_type, None, &mut |buf| {
                    handle
                        .block_on(reader.read(buf))
                        .map_err(|_| storage_error(StorageError::Network))
                })
            }
        }
    }

    /// Seals what `read` gives into a new content object at the target and writes its sidecar;
    /// a file already at `rel` (the user chose to replace it) is replaced after that. `expected`
    /// is the SHA-256 the plaintext must have (a copy out of another encrypted folder).
    fn seal_upload(
        &mut self,
        rel: &[String],
        modified: u64,
        content_type: Option<String>,
        expected: Option<[u8; 32]>,
        read: &mut Reader<'_>,
    ) -> Result<(), Stop> {
        let target = self.enc_target();
        let (files, folder) = (Arc::clone(&target.files), Arc::clone(&target.folder));
        let (dirs, name) = rel.split_at(rel.len().saturating_sub(1));
        let name = name.first().cloned().unwrap_or_default();
        let parent = self.encrypted_folder(dirs)?;
        let replaced = self.replaced_at(rel)?;
        let oid = new_id();
        let key = folder.content_key(&oid);
        let sealed = self.seal_content(&files, &key, read)?;
        if expected.is_some_and(|sha| sha != sealed.sha256) {
            self.delete_quietly(&files, &key);
            return Err(Stop::Failed(damaged()));
        }
        let Sealed {
            dek,
            cid,
            size,
            sha256,
        } = sealed;
        let file = NewFile {
            oid,
            cid,
            dek,
            size,
            sha256,
            modified,
            content_type,
        };
        self.place_file(&files, &folder, parent, name, file, replaced)
    }

    /// Writes the sidecar of a complete content object as `name` below `parent`, then drops the
    /// file it replaces (it becomes that file's next version). A cancel before the sidecar leaves
    /// nothing visible (FR-016).
    fn place_file(
        &mut self,
        files: &StorageFiles,
        folder: &OpenFolder,
        parent: Id,
        name: String,
        file: NewFile,
        replaced: Option<Version>,
    ) -> Result<(), Stop> {
        let key = folder.content_key(&file.oid);
        if let Err(stop) = self.check() {
            self.delete_quietly(files, &key);
            return Err(stop);
        }
        let (entry, base, revision) = match &replaced {
            Some(old) => (old.sidecar.entry, Some(old.sid), old.sidecar.revision + 1),
            None => (new_id(), None, 1),
        };
        let version = Version {
            sid: new_id(),
            sidecar: Sidecar {
                entry,
                parent,
                name,
                revision,
                base,
                written: now_ms(),
                kind: SidecarKind::File(FileFacts {
                    size: file.size,
                    modified: file.modified,
                    content_type: file.content_type,
                    sha256: file.sha256,
                    content: file.oid,
                    cid: file.cid,
                    dek: folder.wrap_dek(&entry, &file.cid, &file.dek),
                }),
            },
        };
        let object_len = crate::files::encrypted::format::content::ciphertext_len(file.size);
        let written = self.handle.block_on(write_version(
            files,
            folder,
            version,
            Some((file.oid, object_len)),
        ));
        if let Err(error) = written {
            self.delete_quietly(files, &key);
            return Err(Stop::Failed(error));
        }
        if replaced.is_some() {
            self.handle.block_on(drop_replaced(files, folder, &entry));
        }
        Ok(())
    }

    fn delete_quietly(&self, files: &StorageFiles, key: &str) {
        let _ = self
            .handle
            .block_on(files.store().delete(files.access(), key, soon()));
    }

    /// Seals `read` into the content object `key`, part by part.
    fn seal_content(
        &mut self,
        files: &StorageFiles,
        key: &str,
        read: &mut Reader<'_>,
    ) -> Result<Sealed, Stop> {
        let mut parts = SealedParts::fresh(PART);
        let mut upload = Upload::new(key);
        let mut buf = vec![0u8; CHUNK];
        let sent = (|| {
            loop {
                self.check()?;
                let got = read(&mut buf)?;
                if got == 0 {
                    break;
                }
                for body in parts.push(&buf[..got]) {
                    self.send_part(files, &mut upload, body)?;
                }
                self.done.bytes += got as u64;
                self.report();
            }
            let (last, sealed) = parts.finish();
            self.finish_upload(files, &mut upload, last)?;
            Ok(sealed)
        })();
        if sent.is_err() {
            self.abort_upload(files, &upload);
        }
        sent
    }
}

#[cfg(test)]
#[path = "remote_encrypted_tests.rs"]
mod tests;
