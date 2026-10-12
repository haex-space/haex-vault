//! The encrypted halves of transfers (spec 048 US1, research R9): [`SealedParts`] seals plaintext
//! into a content object in upload parts, [`Decrypting`] reads a file of an encrypted folder window
//! by window and checks its SHA-256 at the end, [`ensure_folder`] makes the sub folders a
//! transfer into an encrypted folder needs, [`file_version`] is the sidecar of a new file, and
//! [`put_file`] writes a whole file from memory (the bridge of extensions, spec 048 US6).
//!
//! Every block is sealed exactly once: a part that fails is sent again with the same bytes, and a
//! transfer that starts again seals a new content object with a new `DEK`, `cid` and
//! `nonce_prefix` (format "Inhaltsobjekt").

use std::sync::Arc;

use sha2::{Digest, Sha256};
use tokio::runtime::Handle;
use zeroize::Zeroizing;

use super::format::content::{ciphertext_len, ContentSealer};
use super::format::sidecar::{FileFacts, Sidecar, SidecarKind};
use super::format::Id;
use super::manage::drop_replaced;
use super::ops::{now_ms, write_version};
use super::route::{open, view, Located};
use super::source::{EncryptedFile, WINDOW};
use super::tree::{facts, Item, Version};
use super::{damaged, new_id, OpenFolder};
use crate::files::kind::mime_for;
use crate::files::local::edit::check_name;
use crate::files::storage_source::{storage_error, StorageFiles};
use crate::files::{FilesError, FilesErrorCode};

fn soon() -> tokio::time::Instant {
    tokio::time::Instant::now() + std::time::Duration::from_secs(30)
}

/// What a sealed upload produced, for its sidecar.
pub struct Sealed {
    pub dek: Zeroizing<[u8; 32]>,
    pub cid: Id,
    /// Plaintext bytes.
    pub size: u64,
    pub sha256: [u8; 32],
}

/// Seals one file into parts of `part` bytes of ciphertext (the last one shorter).
pub struct SealedParts {
    sealer: ContentSealer,
    buffer: Vec<u8>,
    sha: Sha256,
    size: u64,
    part: usize,
}

impl SealedParts {
    /// A new content object: fresh `DEK`, `cid` and `nonce_prefix`.
    pub fn fresh(part: usize) -> Self {
        Self {
            sealer: ContentSealer::fresh(),
            buffer: Vec::with_capacity(part + 2 * crate::files::encrypted::format::CHUNK),
            sha: Sha256::new(),
            size: 0,
            part,
        }
    }

    /// Takes plaintext; returns the parts that are full now.
    pub fn push(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        self.sha.update(data);
        self.size += data.len() as u64;
        self.sealer.push(data, &mut self.buffer);
        let mut full = Vec::new();
        while self.buffer.len() >= self.part {
            let rest = self.buffer.split_off(self.part);
            full.push(std::mem::replace(&mut self.buffer, rest));
        }
        full
    }

    /// Seals the last block; returns the last part (never empty) and what the sidecar needs.
    pub fn finish(mut self) -> (Vec<u8>, Sealed) {
        let dek = Zeroizing::new(*self.sealer.dek());
        let cid = self.sealer.cid();
        self.sealer.finish(&mut self.buffer);
        let sealed = Sealed {
            dek,
            cid,
            size: self.size,
            sha256: self.sha.finalize().into(),
        };
        (self.buffer, sealed)
    }
}

impl Sealed {
    /// The length of the content object.
    pub fn object_len(&self) -> u64 {
        ciphertext_len(self.size)
    }
}

/// Reads a whole file of an encrypted folder on a blocking thread, one window per request, and
/// checks its SHA-256 against the sidecar at the end (format: "beim vollständigen Lesen").
pub struct Decrypting {
    file: Arc<EncryptedFile>,
    handle: Handle,
    expected: [u8; 32],
    sha: Sha256,
    window: Vec<u8>,
    taken: usize,
    next: u64,
}

impl Decrypting {
    pub fn new(file: EncryptedFile, expected: [u8; 32], handle: Handle) -> Self {
        Self {
            file: Arc::new(file),
            handle,
            expected,
            sha: Sha256::new(),
            window: Vec::new(),
            taken: 0,
            next: 0,
        }
    }

    /// Like `Read::read`; a failed check reads as an error.
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, FilesError> {
        if self.taken == self.window.len() {
            let size = self.file.size();
            if self.next >= size {
                return Ok(0);
            }
            let end = size.min(self.next + WINDOW);
            self.window = self.handle.block_on(self.file.read(self.next, end))?;
            self.sha.update(&self.window);
            self.taken = 0;
            self.next = end;
        }
        let n = buf.len().min(self.window.len() - self.taken);
        buf[..n].copy_from_slice(&self.window[self.taken..self.taken + n]);
        self.taken += n;
        Ok(n)
    }

    /// Whether everything read matches the sidecar's SHA-256.
    pub fn finish(self) -> Result<(), FilesError> {
        let sha: [u8; 32] = self.sha.finalize().into();
        if self.next == self.file.size() && sha == self.expected {
            Ok(())
        } else {
            Err(damaged())
        }
    }
}

/// The entry id of the folder at `rel` below the encrypted folder's node `base`, made (one sidecar
/// each) where it does not exist yet. A file in the way is `exists`.
pub async fn ensure_folder(
    files: &StorageFiles,
    folder: &OpenFolder,
    base: &[String],
    rel: &[String],
) -> Result<Id, FilesError> {
    let mut path = base.to_vec();
    let mut parent = {
        let view = view(files, folder).await?;
        let index = view
            .tree
            .find(&path)
            .ok_or_else(|| FilesError::new(FilesErrorCode::NotFound, "not found"))?;
        view.tree
            .parent_id(index)
            .ok_or_else(|| FilesError::invalid_path("nothing can be created here"))?
    };
    for name in rel {
        path.push(name.clone());
        let found = {
            let view = view(files, folder).await?;
            view.tree
                .find(&path)
                .map(|index| (view.tree.is_dir(index), view.tree.parent_id(index)))
        };
        parent = match found {
            Some((true, Some(id))) => id,
            Some(_) => return Err(FilesError::new(FilesErrorCode::Exists, "the name is taken")),
            None => {
                let entry = new_id();
                let version = Version {
                    sid: new_id(),
                    sidecar: Sidecar {
                        entry,
                        parent,
                        name: name.clone(),
                        revision: 1,
                        base: None,
                        written: now_ms(),
                        kind: SidecarKind::Folder,
                    },
                };
                write_version(files, folder, version, None).await?;
                entry
            }
        };
    }
    Ok(parent)
}

/// A content object complete at the target, for its sidecar.
pub struct NewFile {
    pub oid: Id,
    pub cid: Id,
    pub dek: Zeroizing<[u8; 32]>,
    pub size: u64,
    pub sha256: [u8; 32],
    pub modified: u64,
    pub content_type: Option<String>,
}

/// The sidecar of `file` as `name` below `parent`: a new entry, or the next version of `replaced`
/// (format "Schreiben").
pub fn file_version(
    folder: &OpenFolder,
    parent: Id,
    name: String,
    file: NewFile,
    replaced: Option<&Version>,
) -> Version {
    let (entry, base, revision) = match replaced {
        Some(old) => (old.sidecar.entry, Some(old.sid), old.sidecar.revision + 1),
        None => (new_id(), None, 1),
    };
    Version {
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
    }
}

/// Writes `data` as the file at `at` (names below the encrypted folder): missing folders get a
/// sidecar each, a file there becomes the next version, a folder there is `exists`. The content
/// object goes in one request, then the sidecar; nothing shows before it (FR-016).
pub async fn put_file(files: &StorageFiles, at: &Located, data: &[u8]) -> Result<(), FilesError> {
    let Some((name, dirs)) = at.rest.split_last() else {
        return Err(FilesError::invalid_path("the encrypted folder is no file"));
    };
    for part in &at.rest {
        check_name(part)?;
    }
    let folder = open(files, at).await?;
    let parent = ensure_folder(files, &folder, &[], dirs).await?;
    let replaced = {
        let view = view(files, &folder).await?;
        match view
            .tree
            .find(&at.rest)
            .map(|index| &view.tree.node(index).item)
        {
            None => None,
            Some(Item::Version { version, .. }) if facts(version).is_some() => {
                Some((**version).clone())
            }
            Some(_) => return Err(FilesError::new(FilesErrorCode::Exists, "the name is taken")),
        }
    };
    // One part for the whole object: it is in memory already.
    let whole = usize::try_from(ciphertext_len(data.len() as u64))
        .map_err(|_| FilesError::new(FilesErrorCode::TooLarge, "too large"))?;
    let mut sealer = SealedParts::fresh(whole);
    let mut body = sealer.push(data).concat();
    let (last, sealed) = sealer.finish();
    body.extend_from_slice(&last);
    let oid = new_id();
    let key = folder.content_key(&oid);
    files
        .store()
        .put(files.access(), &key, body, soon())
        .await
        .map_err(storage_error)?;
    let object_len = sealed.object_len();
    let file = NewFile {
        oid,
        cid: sealed.cid,
        size: sealed.size,
        sha256: sealed.sha256,
        dek: sealed.dek,
        modified: now_ms(),
        content_type: mime_for(name).map(str::to_owned),
    };
    let version = file_version(&folder, parent, name.clone(), file, replaced.as_ref());
    let entry = version.sidecar.entry;
    if let Err(error) = write_version(files, &folder, version, Some((oid, object_len))).await {
        let _ = files.store().delete(files.access(), &key, soon()).await;
        return Err(error);
    }
    if replaced.is_some() {
        drop_replaced(files, &folder, &entry).await;
    }
    Ok(())
}

#[cfg(test)]
#[path = "transfer_tests.rs"]
mod tests;
