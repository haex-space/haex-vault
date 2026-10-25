//! An upload into a share of a storage (spec 056 T081): the parts go on to the provider in order,
//! into an encrypted folder sealed on the way (spec 048, as a transfer seals). What fits into one
//! part goes as one object; more starts an upload in parts, so every part but the last is full. In
//! an encrypted folder nothing shows before its sidecar, written last (FR-016); a dropped upload
//! leaves no part at the provider.

use std::sync::Arc;

use super::storage::{fail_of, provider, soon, InStorage};
use crate::files::encrypted::format::content::ciphertext_len;
use crate::files::encrypted::format::Id;
use crate::files::encrypted::manage::drop_replaced;
use crate::files::encrypted::new_id;
use crate::files::encrypted::ops::{now_ms, write_version};
use crate::files::encrypted::route::{open, route, view, Located, Route};
use crate::files::encrypted::transfer::{ensure_folder, file_version, NewFile, SealedParts};
use crate::files::encrypted::tree::{facts, Item};
use crate::files::encrypted::OpenFolder;
use crate::files::kind::mime_for;
use crate::files::local::edit::check_name;
use crate::files::peer::wire::FailCode;
use crate::files::storage_source::{object_key, StorageFiles};
use crate::files::transfer::remote::PART;

/// The bytes of one object at the provider, sent as they come.
struct Provider {
    key: String,
    /// The body not sent yet: the last one may be shorter, so it waits for the next.
    pending: Option<Vec<u8>>,
    /// Set once an upload in parts began.
    id: Option<String>,
    marks: Vec<String>,
}

impl Provider {
    fn new(key: String) -> Self {
        Self {
            key,
            pending: None,
            id: None,
            marks: Vec::new(),
        }
    }

    async fn send(&mut self, files: &StorageFiles, body: Vec<u8>) -> Result<(), FailCode> {
        if let Some(waiting) = self.pending.replace(body) {
            self.part(files, waiting).await?;
        }
        Ok(())
    }

    async fn part(&mut self, files: &StorageFiles, body: Vec<u8>) -> Result<(), FailCode> {
        let (store, access) = (files.store(), files.access());
        let id = match &self.id {
            Some(id) => id.clone(),
            None => {
                let id = store
                    .begin_upload(access, &self.key, soon())
                    .await
                    .map_err(provider)?;
                self.id = Some(id.clone());
                id
            }
        };
        let number = u16::try_from(self.marks.len() + 1).map_err(|_| FailCode::TooLarge)?;
        let mark = store
            .upload_part(access, &self.key, &id, number, body, soon())
            .await
            .map_err(provider)?;
        self.marks.push(mark);
        Ok(())
    }

    /// The object complete at the provider.
    async fn finish(&mut self, files: &StorageFiles) -> Result<(), FailCode> {
        let last = self.pending.take().unwrap_or_default();
        let (store, access) = (files.store(), files.access());
        match self.id.clone() {
            None => store
                .put(access, &self.key, last, soon())
                .await
                .map_err(provider),
            Some(id) => {
                if !last.is_empty() {
                    self.part(files, last).await?;
                }
                store
                    .finish_upload(access, &self.key, &id, &self.marks, soon())
                    .await
                    .map_err(provider)
            }
        }
    }

    async fn abort(&mut self, files: &StorageFiles) {
        if let Some(id) = self.id.take() {
            let _ = files
                .store()
                .abort_upload(files.access(), &self.key, &id, soon())
                .await;
        }
    }
}

/// Into an encrypted folder: the sealer, the content object's id, and where the file goes.
struct Sealing {
    parts: SealedParts,
    oid: Id,
    folder: Arc<OpenFolder>,
    at: Located,
}

/// One upload into a storage.
pub struct StorageUpload {
    target: InStorage,
    provider: Provider,
    sealing: Option<Sealing>,
}

impl StorageUpload {
    /// Starts writing the file `target`; nothing reaches the provider before the first full part.
    pub async fn begin(target: InStorage) -> Result<Self, FailCode> {
        if target.at.key.is_empty() {
            return Err(FailCode::Refused);
        }
        let files = Arc::clone(&target.files);
        match route(&files, &target.path()).await.map_err(fail_of)? {
            Route::Encrypted(at) => {
                if at.rest.is_empty() {
                    return Err(FailCode::Refused);
                }
                for name in &at.rest {
                    check_name(name).map_err(fail_of)?;
                }
                let folder = open(&files, &at).await.map_err(fail_of)?;
                let oid = new_id();
                Ok(Self {
                    provider: Provider::new(folder.content_key(&oid)),
                    sealing: Some(Sealing {
                        parts: SealedParts::fresh(PART),
                        oid,
                        folder,
                        at,
                    }),
                    target,
                })
            }
            Route::Plain => Ok(Self {
                provider: Provider::new(object_key(&target.path()).map_err(fail_of)?),
                sealing: None,
                target,
            }),
        }
    }

    /// Takes the next part.
    pub async fn part(&mut self, body: Vec<u8>) -> Result<(), FailCode> {
        let files = Arc::clone(&self.target.files);
        let bodies = match &mut self.sealing {
            Some(sealing) => sealing.parts.push(&body),
            // The other side sends full parts but the last; they go on as they are.
            None => vec![body],
        };
        for body in bodies {
            self.provider.send(&files, body).await?;
        }
        Ok(())
    }

    /// Puts the file in place; in an encrypted folder its sidecar shows it, as the next version of
    /// a file there.
    pub async fn finish(mut self) -> Result<(), FailCode> {
        let files = Arc::clone(&self.target.files);
        let Some(sealing) = self.sealing.take() else {
            let done = self.provider.finish(&files).await;
            if done.is_err() {
                self.provider.abort(&files).await;
            }
            return done;
        };
        let (last, sealed) = sealing.parts.finish();
        let written = async {
            self.provider.send(&files, last).await?;
            self.provider.finish(&files).await
        };
        if let Err(code) = written.await {
            self.provider.abort(&files).await;
            return Err(code);
        }
        let file = NewFile {
            oid: sealing.oid,
            cid: sealed.cid,
            size: sealed.size,
            sha256: sealed.sha256,
            dek: sealed.dek,
            modified: now_ms(),
            content_type: sealing
                .at
                .rest
                .last()
                .and_then(|name| mime_for(name))
                .map(str::to_owned),
        };
        let placed = place(&files, &sealing.folder, &sealing.at, file).await;
        if placed.is_err() {
            let _ = files
                .store()
                .delete(files.access(), &self.provider.key, soon())
                .await;
        }
        placed
    }

    /// Drops what was sent so far.
    pub async fn abort(mut self) {
        let files = Arc::clone(&self.target.files);
        self.provider.abort(&files).await;
    }
}

/// Writes the sidecar of the complete content object `file` at `at`: missing folders get one
/// each, a file there is replaced by its next version, a folder there is `NameTaken`.
async fn place(
    files: &StorageFiles,
    folder: &OpenFolder,
    at: &Located,
    file: NewFile,
) -> Result<(), FailCode> {
    let Some((name, dirs)) = at.rest.split_last() else {
        return Err(FailCode::Refused);
    };
    let parent = ensure_folder(files, folder, &[], dirs)
        .await
        .map_err(fail_of)?;
    let replaced = {
        let view = view(files, folder).await.map_err(fail_of)?;
        match view
            .tree
            .find(&at.rest)
            .map(|index| &view.tree.node(index).item)
        {
            None => None,
            Some(Item::Version { version, .. }) if facts(version).is_some() => {
                Some((**version).clone())
            }
            Some(_) => return Err(FailCode::NameTaken),
        }
    };
    let (oid, object_len) = (file.oid, ciphertext_len(file.size));
    let version = file_version(folder, parent, name.clone(), file, replaced.as_ref());
    let entry = version.sidecar.entry;
    write_version(files, folder, version, Some((oid, object_len)))
        .await
        .map_err(fail_of)?;
    if replaced.is_some() {
        drop_replaced(files, folder, &entry).await;
    }
    Ok(())
}
