//! The encrypted halves of transfers (spec 048 US1, research R9): [`SealedParts`] seals plaintext
//! into a content object in upload parts, [`Decrypting`] reads a file of an encrypted folder window
//! by window and checks its SHA-256 at the end, and [`ensure_folder`] makes the sub folders a
//! transfer into an encrypted folder needs.
//!
//! Every block is sealed exactly once: a part that fails is sent again with the same bytes, and a
//! transfer that starts again seals a new content object with a new `DEK`, `cid` and
//! `nonce_prefix` (format "Inhaltsobjekt").

use std::sync::Arc;

use sha2::{Digest, Sha256};
use tokio::runtime::Handle;
use zeroize::Zeroizing;

use super::format::content::{ciphertext_len, ContentSealer};
use super::format::sidecar::{Sidecar, SidecarKind};
use super::format::Id;
use super::ops::{now_ms, write_version};
use super::route::view;
use super::source::{EncryptedFile, WINDOW};
use super::tree::Version;
use super::{damaged, new_id, OpenFolder};
use crate::files::storage_source::StorageFiles;
use crate::files::{FilesError, FilesErrorCode};

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

#[cfg(test)]
#[path = "transfer_tests.rs"]
mod tests;
