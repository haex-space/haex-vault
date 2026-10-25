//! The checks before a request runs (spec 056 FR-004, FR-005, FR-007): what holds now for the
//! connection's device, the share it names, the path and holzi's own places.

use super::roots::{self, Located};
use super::{check, ops, Peer, ServeContext};
use crate::files::peer::handshake;
use crate::files::peer::wire::{FailCode, Mode, Request};
use crate::files::share::store::{self, FileShareRow};
use crate::files::share::{device_access, served, FileShareLevel};
use crate::remote_storage::local_fs::resolve;

/// Checks a request about the upload `upload_id` again, with the root and path it began with.
pub(super) async fn again(
    ctx: &ServeContext,
    peer: &Peer,
    request: &Request,
    upload_id: &str,
) -> Result<(), FailCode> {
    let upload = peer.uploads.get(upload_id).ok_or(FailCode::NotFound)?;
    let checked = locate(ctx, peer, request, upload.share.as_deref(), &upload.path)
        .await
        .map(|_| ());
    if checked == Err(FailCode::Refused) {
        // No longer allowed (FR-024): what was sent so far goes.
        let _ = peer.uploads.abort(ctx, upload_id).await;
    }
    checked
}

/// Checks `request` against what holds now (FR-005) and returns the share it names with the level
/// of the asking vault.
pub(super) async fn permit(
    ctx: &ServeContext,
    peer: &Peer,
    request: &Request,
    share: Option<&str>,
) -> Result<Option<(FileShareRow, FileShareLevel)>, FailCode> {
    let grant = match peer.mode {
        Mode::OwnDevice => {
            let on = ctx
                .db
                .read(|q| Ok(device_access::enabled(q)?))
                .await
                .map_err(|_| FailCode::Internal)?;
            if !on {
                return Err(FailCode::Refused);
            }
            None
        }
        Mode::AccessOff => None,
        Mode::Contact => {
            if !handshake::still_contact(&ctx.db, peer.vault).await {
                return Err(FailCode::Refused);
            }
            match share {
                Some(id) => grant_of(ctx, peer.vault, id).await?,
                None => None,
            }
        }
    };
    check::allowed(
        peer.mode,
        &request.op,
        grant.as_ref().map(|(row, level)| (row, *level)),
    )?;
    Ok(grant)
}

/// The share `id` if this device serves it and `vault` holds a grant in it.
pub(super) async fn grant_of(
    ctx: &ServeContext,
    vault: [u8; 32],
    id: &str,
) -> Result<Option<(FileShareRow, FileShareLevel)>, FailCode> {
    let (id, device, vault) = (
        id.to_owned(),
        ctx.device.clone(),
        crate::sync::keys::hex(&vault),
    );
    ctx.db
        .read(move |q| {
            let Some(row) = store::get_share(q, &id)? else {
                return Ok(None);
            };
            if row.vault_device_uuid != device {
                return Ok(None);
            }
            Ok(store::grant_for(q, &id, &vault)?.map(|level| (row, level)))
        })
        .await
        .map_err(|_| FailCode::Internal)
}

/// The target of `path` after every check: the request may run, the path stays in its root, and
/// holzi's own places are kept as FR-007 says.
pub(super) async fn locate(
    ctx: &ServeContext,
    peer: &Peer,
    request: &Request,
    share: Option<&str>,
    path: &str,
) -> Result<Located, FailCode> {
    let grant = permit(ctx, peer, request, share).await?;
    let at = match (peer.mode, &grant) {
        (Mode::OwnDevice, None) => roots::on_device(path)?,
        (Mode::Contact, Some((row, _))) => roots::in_share(row, path)?,
        _ => return Err(FailCode::Refused),
    };
    check::at_root(&request.op, &at.key)?;
    // Inside its root, links resolved; a new name resolves to where it would be.
    let resolved = resolve(&at.root, &at.key).map_err(ops::fail_of);
    if let Some((row, _)) = &grant {
        served::note_source(&ctx.db, row, resolved.is_ok()).await;
    }
    let real = resolved?;
    let writes = request.op.writes();
    let own = ctx.own.touches(&real, writes) || ctx.own.contains_resolved(&at.path());
    check::own_data(peer.mode, request.caller, writes, own)?;
    Ok(at)
}
