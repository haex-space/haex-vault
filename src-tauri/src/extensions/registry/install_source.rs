//! Where the bytes of an install come from (spec 069, FR-010, contracts/commands.md): a file the
//! user picked, or a bundle the host downloaded from a marketplace and keeps in memory.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::install::{
    install, install_preview, read_bundle_file, InstallPreview, Installed, PermissionChoice,
};
use crate::error::Result;
use crate::files::picked::{Opener, PickedFile};
use crate::marketplace::pending::PendingSlot;
use crate::vault_gate::VaultDb;

/// The source of a bundle for `extension_install_preview` and `extension_install`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum BundleSource {
    /// The choice of the open dialog (spec 043: a path or a provider address).
    File { file: PickedFile },
    /// A bundle downloaded by `marketplace_download`; `id` is its `PendingBundleRef.id`.
    Marketplace { id: String },
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ExtensionInstallArgs {
    /// Where the bytes come from: the choice of the open dialog (spec 043: a path or a provider
    /// address) or a bundle downloaded from a marketplace (spec 069).
    pub source: BundleSource,
    /// The choice per declared permission; a declaration without a choice becomes `ask`.
    pub accepted: Vec<PermissionChoice>,
    /// The user confirmed a downgrade or the replacement of another bundle of the same version.
    #[serde(default)]
    pub confirmed: bool,
}

/// Where a previewed bundle came from, when it did not come from a file (FR-011).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct InstallOrigin {
    /// The name of the marketplace as the user knows it.
    pub source_name: String,
}

/// The bytes of a source and what is known about where they came from.
#[derive(Debug, Clone)]
pub struct ResolvedBundle {
    pub bytes: Arc<Vec<u8>>,
    pub origin: Option<InstallOrigin>,
}

/// Reads the bytes of `source`: a file through the opener (checked like before, limit included),
/// a marketplace bundle from the pending slot, which counts as its use. Blocking.
pub fn read_bundle(
    opener: &impl Opener,
    pending: &PendingSlot,
    source: &BundleSource,
) -> Result<ResolvedBundle> {
    match source {
        BundleSource::File { file } => Ok(ResolvedBundle {
            bytes: Arc::new(read_bundle_file(opener, file)?),
            origin: None,
        }),
        BundleSource::Marketplace { id } => {
            let bundle = pending.peek(id)?;
            Ok(ResolvedBundle {
                bytes: Arc::clone(&bundle.bytes),
                origin: Some(InstallOrigin {
                    source_name: bundle.source_name.clone(),
                }),
            })
        }
    }
}

/// The preview of the bundle of `source`; writes nothing. A marketplace bundle names its origin.
pub fn preview_source(
    db: &VaultDb,
    opener: &impl Opener,
    pending: &PendingSlot,
    source: &BundleSource,
) -> Result<InstallPreview> {
    let bundle = read_bundle(opener, pending, source)?;
    let bytes = bundle.bytes;
    let mut preview = db.read_blocking(move |q| install_preview(q, &bytes).map_err(Into::into))?;
    preview.origin = bundle.origin;
    Ok(preview)
}

/// Installs the bundle of `source`. A marketplace bundle leaves the pending slot after a
/// successful install only: a refused one stays, so the preview can be tried again.
pub fn install_source(
    db: &VaultDb,
    opener: &impl Opener,
    pending: &PendingSlot,
    args: ExtensionInstallArgs,
    device: uuid::Uuid,
    now_ms: i64,
) -> Result<Installed> {
    let bundle = read_bundle(opener, pending, &args.source)?;
    let installed = install(
        db,
        &bundle.bytes,
        args.accepted,
        args.confirmed,
        device,
        now_ms,
    )?;
    if let BundleSource::Marketplace { id } = &args.source {
        pending.take(id);
    }
    Ok(installed)
}

#[cfg(test)]
#[path = "install_marketplace_tests.rs"]
mod install_marketplace_tests;
