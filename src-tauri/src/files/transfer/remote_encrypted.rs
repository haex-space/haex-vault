//! The encrypted halves of a transfer across sources (spec 048 US1, T027): into an encrypted folder
//! the bytes are sealed on the way (content object first, then the sidecar, then the replaced
//! version goes; nothing shows before its sidecar), out of one they are opened and checked against
//! the sidecar's SHA-256 before the part file becomes the file. Taken names are asked about with the
//! same hooks as everywhere (by their decrypted names).

use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

use tokio::io::AsyncReadExt;

use super::upload::Upload;
use super::{soon, Place, Run, Side, Stop, PART};
use crate::files::encrypted::format::sidecar::{FileFacts, Sidecar, SidecarKind};
use crate::files::encrypted::ops::{later, now_ms, write_version};
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

impl<'h> Run<'h, '_> {
    fn enc_target(&self) -> &'h EncTarget {
        self.target
            .expect("an encrypted place has an encrypted target")
    }

    /// The target folder's names and then `rel`.
    fn enc_path(&self, rel: &[String]) -> Vec<String> {
        let mut path = self.enc_target().rest.clone();
        path.extend_from_slice(rel);
        path
    }

    /// What lies at `rel` below the target: a file version, a folder, or nothing.
    fn enc_lookup(&self, rel: &[String]) -> Result<Option<Option<Version>>, Stop> {
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

    /// Makes the folder `rel` below the target (one sidecar per missing folder).
    pub(super) fn encrypted_folder(&self, rel: &[String]) -> Result<(), Stop> {
        let target = self.enc_target();
        self.handle.block_on(ensure_folder(
            &target.files,
            &target.folder,
            &target.rest,
            rel,
        ))?;
        Ok(())
    }

    /// Sends a file when one end is an encrypted folder.
    pub(super) fn send_encrypted(&mut self, item: &Item, place: &Place) -> Result<(), Stop> {
        match (&item.encrypted, place) {
            (Some(source), Place::Device(path)) => {
                let reading = self.open_source(source)?;
                self.download(path, move |run, part| run.decrypt_into(reading, part))
            }
            (Some(source), Place::Encrypted(rel)) => {
                let mut reading = self.open_source(source)?;
                let version = source.version.as_ref().and_then(facts);
                let (modified, content_type, expected) = version
                    .map(|f| (f.modified, f.content_type.clone(), Some(f.sha256)))
                    .unwrap_or((now_ms(), None, None));
                self.seal_upload(rel, modified, content_type, expected, &mut |buf| {
                    reading.read(buf)
                })
            }
            (None, Place::Encrypted(rel)) => self.plain_into_encrypted(item, rel),
            _ => Err(Stop::Failed(later())),
        }
    }

    /// Opens the file of `source` for reading through.
    fn open_source(&self, source: &EncSource) -> Result<Decrypting, Stop> {
        let Side::Storage(files) = &self.job.from else {
            return Err(Stop::Failed(later()));
        };
        let version = source
            .version
            .as_ref()
            .ok_or_else(|| FilesError::invalid_path("not a file"))?;
        let expected = facts(version).map(|f| f.sha256).unwrap_or_default();
        let file = self.handle.block_on(EncryptedFile::open(
            files,
            Arc::clone(&source.folder),
            version,
        ))?;
        Ok(Decrypting::new(file, expected, self.handle.clone()))
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
        let parent = self
            .handle
            .block_on(ensure_folder(&files, &folder, &target.rest, dirs))?;
        let replaced = match self.enc_lookup(rel)? {
            Some(None) => {
                return Err(Stop::Failed(FilesError::new(
                    FilesErrorCode::Exists,
                    "a folder has this name",
                )))
            }
            Some(Some(version)) => Some(version),
            None => None,
        };
        let oid = new_id();
        let key = folder.content_key(&oid);
        let sealed = self.seal_content(&files, &key, read)?;
        let delete_content = |run: &Self| {
            let _ = run
                .handle
                .block_on(files.store().delete(files.access(), &key, soon()));
        };
        if expected.is_some_and(|sha| sha != sealed.sha256) {
            delete_content(self);
            return Err(Stop::Failed(damaged()));
        }
        // A cancel before the sidecar leaves nothing visible (FR-016).
        if let Err(stop) = self.check() {
            delete_content(self);
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
                    size: sealed.size,
                    modified,
                    content_type,
                    sha256: sealed.sha256,
                    content: oid,
                    cid: sealed.cid,
                    dek: folder.wrap_dek(&entry, &sealed.cid, &sealed.dek),
                }),
            },
        };
        let written = self.handle.block_on(write_version(
            &files,
            &folder,
            version,
            Some((oid, sealed.object_len())),
        ));
        if let Err(error) = written {
            delete_content(self);
            return Err(Stop::Failed(error));
        }
        if let Some(old) = replaced {
            self.drop_version(&files, &folder, &old);
        }
        Ok(())
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

    /// Deletes a replaced version after its successor is complete: its content object, then its
    /// sidecar (format "Schreiben"). A failure leaves it for the cleanup (US5).
    fn drop_version(&self, files: &StorageFiles, folder: &OpenFolder, old: &Version) {
        let (store, access) = (files.store(), files.access());
        if let Some(facts) = facts(old) {
            let _ = self.handle.block_on(store.delete(
                access,
                &folder.content_key(&facts.content),
                soon(),
            ));
        }
        if self
            .handle
            .block_on(store.delete(access, &folder.sidecar_key(&old.sid), soon()))
            .is_ok()
        {
            if let Ok(mut view) = self.handle.block_on(view(files, folder)) {
                view.remove(folder.fid, &old.sid);
            }
        }
    }
}
