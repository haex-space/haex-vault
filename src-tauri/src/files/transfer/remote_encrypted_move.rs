//! Moves out of and inside encrypted folders (spec 048 US5, T044):
//!
//! - Inside one encrypted folder an entry is written anew under its new parent, one sidecar, no
//!   content object (FR-018); a taken file name is asked about as everywhere.
//! - Out of one, a moved file goes with all its versions once its copy is complete; the folders it
//!   leaves go at the end, deepest first, when nothing stayed in them (a skipped file keeps its
//!   folder), and the encrypted folder itself when it was moved and is empty then.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::{Place, Run, Stop};
use crate::files::encrypted::manage::{delete_folder, drop_versions, relink};
use crate::files::encrypted::route::view;
use crate::files::encrypted::tree::Version;
use crate::files::storage_source::StorageFiles;
use crate::files::transfer::remote_plan::{EncNode, EncSource, Item};
use crate::files::{FilesError, FilesErrorCode};

impl Run<'_, '_> {
    /// Moves the entry of `item` inside its own encrypted folder.
    pub(super) fn relink(&mut self, item: &Item, source: &EncSource) -> Result<(), Stop> {
        let version = source
            .version()
            .cloned()
            .ok_or_else(|| FilesError::invalid_path("this folder stays"))?;
        let target = self.enc_target();
        let (files, folder) = (Arc::clone(&target.files), Arc::clone(&target.folder));
        let there = self.handle.block_on(async {
            let view = view(&files, &folder).await?;
            Ok::<_, FilesError>(view.tree.find(&self.enc_path(&item.rel)).map(|index| {
                match &view.tree.node(index).item {
                    crate::files::encrypted::tree::Item::Version { version, .. } => {
                        Some(version.sid)
                    }
                    _ => None,
                }
            }))
        })?;
        let moved = |run: &mut Self| {
            if !item.folder {
                run.done.items += 1;
                run.done.bytes += item.size;
                run.report();
            }
        };
        if there == Some(Some(version.sid)) {
            // Moved onto itself.
            moved(self);
            return Ok(());
        }
        let rel = if item.folder {
            if there.is_some() {
                return Err(Stop::Failed(FilesError::new(
                    FilesErrorCode::Exists,
                    "the name is taken",
                )));
            }
            item.rel.clone()
        } else {
            match self.resolve(&item.rel)? {
                Some(Place::Encrypted(rel)) => rel,
                Some(_) => unreachable!("a place in an encrypted folder"),
                None => {
                    moved(self);
                    return Ok(());
                }
            }
        };
        let replaced = if item.folder {
            None
        } else {
            self.replaced_at(&rel)?
        };
        let (dirs, name) = rel.split_at(rel.len().saturating_sub(1));
        let parent = self.encrypted_folder(dirs)?;
        let name = name.first().cloned().unwrap_or_default();
        self.handle
            .block_on(relink(&files, &folder, &version, parent, &name))?;
        if let Some(old) = replaced {
            self.drop_with_replaced(&files, &folder, &old)?;
        }
        moved(self);
        Ok(())
    }

    /// After a moved item is at the target: a file of an encrypted folder goes with its versions,
    /// a folder is kept for [`Self::finish_move`].
    pub(super) fn remove_encrypted(&mut self, source: &EncSource) -> Result<(), Stop> {
        match &source.node {
            EncNode::File(version) => {
                let files = self.source_files()?;
                self.drop_with_replaced(&files, &source.folder, version)
            }
            _ => {
                self.emptied.push(source.clone());
                Ok(())
            }
        }
    }

    /// The end of a move out of encrypted folders: the folders it left, deepest first, when
    /// nothing stayed in them; a moved encrypted folder itself when it is empty then.
    pub(super) fn finish_move(&mut self) -> Result<(), Stop> {
        let files = self.source_files()?;
        let emptied = std::mem::take(&mut self.emptied);
        for source in emptied.iter().rev() {
            self.check()?;
            let folder = &source.folder;
            let empty = self.handle.block_on(async {
                let view = view(&files, folder).await?;
                Ok::<_, FilesError>(match &source.node {
                    EncNode::Folder(version) => view
                        .tree
                        .find_sid(&version.sid)
                        .map(|index| view.tree.children(index).is_empty()),
                    EncNode::Root => Some(view.tree.children(0).is_empty()),
                    EncNode::Recovered | EncNode::File(_) => None,
                })
            })?;
            match (&source.node, empty) {
                (EncNode::Folder(version), Some(true)) => {
                    self.drop_with_replaced(&files, folder, version)?;
                }
                (EncNode::Root, Some(true)) => {
                    let cancel = self.hooks.cancel.clone();
                    self.handle.block_on(delete_folder(
                        &files,
                        &folder.prefix,
                        Some(folder.fid),
                        &cancel,
                        &mut |_| {},
                    ))?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Deletes `version` with the older versions that would show again without it.
    fn drop_with_replaced(
        &self,
        files: &StorageFiles,
        folder: &crate::files::encrypted::OpenFolder,
        version: &Version,
    ) -> Result<(), Stop> {
        self.handle.block_on(async {
            let doomed = view(files, folder).await?.with_replaced(&[version.sid]);
            drop_versions(
                files,
                folder,
                &doomed,
                &CancellationToken::new(),
                &mut |_| {},
            )
            .await
        })?;
        Ok(())
    }
}
