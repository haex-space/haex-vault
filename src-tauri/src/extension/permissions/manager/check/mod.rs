pub(super) mod database;
pub(super) mod filesystem;
pub(super) mod identities;
mod mail;
mod notifications;
pub(super) mod passwords;
pub(super) mod rw_resource;
pub(super) mod shell;
pub(super) mod spaces;
mod sync_servers;
pub(super) mod web;

use crate::extension::permissions::types::PermissionStatus;

/// Resolves a set of matching permission statuses with **deny-first precedence**.
///
/// Returns:
/// - `Some(Denied)` if ANY input status is `Denied`
/// - else `Some(Granted)` if ANY is `Granted`
/// - else `Some(Ask)` if ANY is `Ask`
/// - else `None` (no matches)
///
/// This prevents a broad grant from masking a more specific deny — the previous
/// `iter().find()` first-match logic made the outcome depend on insertion order.
pub(crate) fn deny_first_precedence(
    statuses: impl IntoIterator<Item = PermissionStatus>,
) -> Option<PermissionStatus> {
    let mut seen_granted = false;
    let mut seen_ask = false;
    for s in statuses {
        match s {
            PermissionStatus::Denied => return Some(PermissionStatus::Denied),
            PermissionStatus::Granted => seen_granted = true,
            PermissionStatus::Ask => seen_ask = true,
        }
    }
    if seen_granted {
        Some(PermissionStatus::Granted)
    } else if seen_ask {
        Some(PermissionStatus::Ask)
    } else {
        None
    }
}
