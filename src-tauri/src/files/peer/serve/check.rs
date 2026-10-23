//! The decision on every request (spec 056 FR-004, FR-005, FR-007): what the mode of the connection,
//! the grant read for this very request and holzi's own places allow. Pure, so every rule is tested
//! without a connection.

use crate::files::peer::wire::{Caller, Expected, FailCode, Mode, Op};
use crate::files::share::store::FileShareRow;
use crate::files::share::{FileShareItemKind, FileShareLevel};

/// Whether `op` may run in `mode` with `grant`, the share and level read for this request (`None`
/// when the request names no share or the share is not granted to the asking vault).
pub fn allowed(
    mode: Mode,
    op: &Op,
    grant: Option<(&FileShareRow, FileShareLevel)>,
) -> Result<(), FailCode> {
    match mode {
        Mode::AccessOff => Err(FailCode::Refused),
        // An own device reaches the whole file system; it names no share.
        Mode::OwnDevice if op.share().is_some() => Err(FailCode::Refused),
        Mode::OwnDevice => Ok(()),
        Mode::Contact => match (op, grant) {
            (Op::Shares, _) => Ok(()),
            (_, None) => Err(FailCode::Refused),
            (op, Some((_, level))) if op.writes() && level != FileShareLevel::ReadWrite => {
                Err(FailCode::Refused)
            }
            (op, Some((share, _))) if share.item_kind == FileShareItemKind::File => on_file(op),
            (_, Some(_)) => Ok(()),
        },
    }
}

/// A share of a single file: reading it, and writing its content over the version the contact saw
/// (contracts/files-protocol.md); nothing is created, moved or deleted there.
fn on_file(op: &Op) -> Result<(), FailCode> {
    match op {
        Op::ListDir { .. }
        | Op::Stat { .. }
        | Op::Read { .. }
        | Op::BeginUpload {
            expected: Expected::Version { .. },
            ..
        }
        | Op::UploadPart { .. }
        | Op::FinishUpload { .. }
        | Op::AbortUpload { .. } => Ok(()),
        _ => Err(FailCode::Refused),
    }
}

/// Whether `op` may touch the root of its place itself (`key` empty): never by moving or deleting
/// it, which would take a share or a drive away (FR-037).
pub fn at_root(op: &Op, key: &str) -> Result<(), FailCode> {
    match op {
        Op::Move { .. } | Op::Delete { .. } | Op::CreateFolder { .. } | Op::BeginUpload { .. }
            if key.is_empty() =>
        {
            Err(FailCode::Refused)
        }
        _ => Ok(()),
    }
}

/// Whether a request about a path in holzi's own places (`own`) may run (FR-007): an own device's
/// user may read there as in the local file browser, an agent and a contact may not reach it at all.
pub fn own_data(mode: Mode, caller: Caller, writes: bool, own: bool) -> Result<(), FailCode> {
    if !own {
        return Ok(());
    }
    match (mode, caller) {
        (Mode::OwnDevice, Caller::User) if !writes => Ok(()),
        _ => Err(FailCode::Refused),
    }
}

/// Whether entries in holzi's own places are left out of a listing: for agents and contacts, who
/// never see them (FR-007; for agents as `access::visible_to` does locally).
pub fn hides_own(mode: Mode, caller: Caller) -> bool {
    !(mode == Mode::OwnDevice && caller == Caller::User)
}

#[cfg(test)]
#[path = "check_tests.rs"]
mod tests;
