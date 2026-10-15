//! The role of an entry (research R8): what the icon rules look at beyond its name.

use super::{IconQuery, IconRole};
use crate::files::kind::{viewer_kind, ViewerKind};
use crate::files::{EncryptedRole, EncryptedState};

/// The role of the entry: a state of encryption or access when it has one, otherwise what it is.
/// The cases are those `EntryIcon.vue` told apart before the sets (spec 044 FR-005, spec 048).
pub fn role_of(query: &IconQuery) -> IconRole {
    if let Some(info) = &query.encrypted {
        match info.state {
            EncryptedState::Damaged => return IconRole::Damaged,
            EncryptedState::OtherVault | EncryptedState::NewerFormat => {
                return if query.is_dir {
                    IconRole::LockedFolder
                } else {
                    IconRole::LockedFile
                };
            }
            EncryptedState::Readable if info.role == EncryptedRole::Folder => {
                return IconRole::EncryptedFolder;
            }
            EncryptedState::Readable => {}
        }
    }
    if query.is_dir {
        return if query.no_access {
            IconRole::FolderNoAccess
        } else {
            IconRole::Folder
        };
    }
    match viewer_kind(&query.name) {
        ViewerKind::Image => IconRole::Image,
        ViewerKind::Video => IconRole::Video,
        ViewerKind::Audio => IconRole::Audio,
        ViewerKind::Pdf => IconRole::Pdf,
        ViewerKind::Text => IconRole::Text,
        ViewerKind::Info => IconRole::File,
    }
}

#[cfg(test)]
#[path = "role_tests.rs"]
mod tests;
