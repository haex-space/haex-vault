//! What a transfer takes from encrypted folders (spec 048 US1, US5, T027, T044):
//!
//! - Out of an encrypted folder: its files and folders by their decrypted names; a damaged file
//!   refuses the transfer before it starts (FR-022).
//! - A move inside one encrypted folder: one item per moved entry, written anew under its new
//!   parent (one sidecar each, FR-018); a folder onto a folder of the same name merges child by
//!   child.
//! - The encrypted folder itself into an ordinary folder of a storage: its objects as they are,
//!   the head last, so it stays an encrypted folder there (spec edge case "Den verschlüsselten
//!   Ordner in einen anderen gewöhnlichen Ordner verschieben").
//! - An encrypted folder inside an ordinary folder that goes to the device or into an encrypted
//!   folder: decrypted like a direct copy out of it, never its objects as files (operator
//!   2026-10-11).

use std::sync::Arc;

use super::remote_plan::{soon, EncNode, EncSource, EncTarget, Item};
use crate::files::encrypted::format::is_folder_prefix;
use crate::files::encrypted::manage::shown_version;
use crate::files::encrypted::route::{located_at, open, view, Located};
use crate::files::encrypted::tree::{facts, Item as TreeItem, Tree};
use crate::files::encrypted::{FolderView, OpenFolder};
use crate::files::storage_source::{folder_prefix, storage_error, StorageFiles};
use crate::files::{FilesError, FilesErrorCode};

/// The most objects under one encrypted folder a transfer takes.
const MAX_OBJECTS: usize = 250_000;

fn not_found() -> FilesError {
    FilesError::new(FilesErrorCode::NotFound, "not found")
}

/// The files and folders of the encrypted folder (or entry in one) at `at`, each placed below
/// `lead` and its own name.
pub(super) async fn encrypted_items(
    files: &StorageFiles,
    at: &Located,
    lead: &[String],
    items: &mut Vec<Item>,
) -> Result<(), FilesError> {
    let folder = open(files, at).await?;
    let view = view(files, &folder).await?;
    let top = view.tree.find(&at.rest).ok_or_else(not_found)?;
    let mut name = lead.to_vec();
    name.push(at.rest.last().unwrap_or(&at.head.display).clone());
    let below = view.tree.below(top).into_iter().map(|(index, rel)| {
        let mut placed = name.clone();
        placed.extend(rel);
        (index, placed)
    });
    for (index, rel) in std::iter::once((top, name.clone())).chain(below) {
        let node = match &view.tree.node(index).item {
            TreeItem::Version { damaged: true, .. } | TreeItem::Broken { .. } => {
                return Err(crate::files::encrypted::damaged())
            }
            TreeItem::Version { version, .. } if facts(version).is_some() => {
                EncNode::File((**version).clone())
            }
            TreeItem::Version { version, .. } => EncNode::Folder((**version).clone()),
            TreeItem::Root => EncNode::Root,
            TreeItem::Recovered => EncNode::Recovered,
        };
        let size = match &node {
            EncNode::File(version) => facts(version).map_or(0, |facts| facts.size),
            _ => 0,
        };
        items.push(Item {
            from: String::new(),
            rel,
            size,
            folder: !matches!(node, EncNode::File(_)),
            encrypted: Some(EncSource {
                folder: Arc::clone(&folder),
                node,
                relink: false,
            }),
        });
    }
    Ok(())
}

/// The entry at `at` moved inside its own encrypted folder below `target` (FR-018).
pub(super) async fn relink_items(
    files: &StorageFiles,
    at: &Located,
    target: &EncTarget,
    items: &mut Vec<Item>,
) -> Result<(), FilesError> {
    let folder = open(files, at).await?;
    let view = view(files, &folder).await?;
    let top = view.tree.find(&at.rest).ok_or_else(not_found)?;
    let name = at.rest.last().cloned().ok_or_else(not_found)?;
    let mut inside = Inside {
        view: &view,
        folder: &folder,
        target: &target.rest,
        items,
    };
    inside.relink(top, vec![name])
}

struct Inside<'a> {
    view: &'a FolderView,
    folder: &'a Arc<OpenFolder>,
    target: &'a [String],
    items: &'a mut Vec<Item>,
}

impl Inside<'_> {
    fn tree(&self) -> &Tree {
        &self.view.tree
    }

    fn relink(&mut self, node: usize, rel: Vec<String>) -> Result<(), FilesError> {
        let version = shown_version(&self.tree().node(node).item)?;
        let mut path = self.target.to_vec();
        path.extend(rel.iter().cloned());
        let is_dir = self.tree().is_dir(node);
        let merges = is_dir
            && self
                .tree()
                .find(&path)
                .is_some_and(|there| there != node && self.tree().is_dir(there));
        if merges {
            // The children move one by one; the emptied folder goes at the end of the move.
            let children: Vec<usize> = self.tree().children(node).to_vec();
            self.items.push(Item {
                from: String::new(),
                rel: rel.clone(),
                size: 0,
                folder: true,
                encrypted: Some(EncSource {
                    folder: Arc::clone(self.folder),
                    node: EncNode::Folder(version),
                    relink: false,
                }),
            });
            for child in children {
                let mut deeper = rel.clone();
                deeper.push(self.tree().node(child).name.clone());
                self.relink(child, deeper)?;
            }
            return Ok(());
        }
        let size = facts(&version).map_or(0, |facts| facts.size);
        let node = if is_dir {
            EncNode::Folder(version)
        } else {
            EncNode::File(version)
        };
        self.items.push(Item {
            from: String::new(),
            rel,
            size,
            folder: is_dir,
            encrypted: Some(EncSource {
                folder: Arc::clone(self.folder),
                node,
                relink: true,
            }),
        });
        Ok(())
    }
}

/// The objects of the encrypted folder at `at` as they are, the head last, for an ordinary folder
/// `target` of a storage. A folder with the same id there already refuses the transfer.
pub(super) async fn raw_items(
    files: &StorageFiles,
    at: &Located,
    to: &StorageFiles,
    target: &str,
    items: &mut Vec<Item>,
) -> Result<(), FilesError> {
    let prefix = &at.head.prefix;
    let name = prefix
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_owned();
    let there = format!("{}{name}/h", folder_prefix(target)?);
    match to.store().head(to.access(), &there, soon()).await {
        Ok(_) => {
            return Err(FilesError::new(
                FilesErrorCode::Exists,
                "this encrypted folder is there already",
            ))
        }
        Err(crate::remote_storage::StorageError::NotFound) => {}
        Err(error) => return Err(storage_error(error)),
    }
    let head = format!("{prefix}h");
    let mut objects = files
        .store()
        .list(files.access(), prefix, MAX_OBJECTS, soon())
        .await
        .map_err(storage_error)?;
    objects.sort_by_key(|object| object.key == head);
    for object in objects {
        let mut rel = vec![name.clone()];
        rel.extend(object.key[prefix.len()..].split('/').map(str::to_owned));
        items.push(Item {
            from: object.key,
            rel,
            size: object.size,
            folder: false,
            encrypted: None,
        });
    }
    Ok(())
}

/// The encrypted folder that holds the object `key` below `prefix`, as its object prefix, if any.
pub(super) fn encrypted_prefix_of(prefix: &str, key: &str) -> Option<String> {
    let rest = key.get(prefix.len()..)?;
    let mut at = prefix.len();
    for part in rest.split('/') {
        at += part.len() + 1;
        if is_folder_prefix(part) && at <= key.len() {
            return Some(key[..at].to_owned());
        }
    }
    None
}

/// The items of the encrypted folders `prefixes` found in an ordinary folder whose items start
/// with `name`: decrypted like a direct copy out of each.
pub(super) async fn nested_items(
    files: &StorageFiles,
    source_prefix: &str,
    name: &str,
    prefixes: &[String],
    items: &mut Vec<Item>,
) -> Result<(), FilesError> {
    for prefix in prefixes {
        let at = located_at(files, prefix).await?;
        let mut lead = vec![name.to_owned()];
        let between = &prefix[source_prefix.len()..];
        let parent = between
            .trim_end_matches('/')
            .rsplit_once('/')
            .map(|(p, _)| p);
        lead.extend(
            parent
                .into_iter()
                .flat_map(|p| p.split('/'))
                .map(str::to_owned),
        );
        encrypted_items(files, &at, &lead, items).await?;
    }
    Ok(())
}
