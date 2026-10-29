//! Extension bundles (`.xt`, format `haextension-bundle/2`).
//!
//! Every rule of the format (archive, paths, canonical JSON, signature, manifest, migrations) lives
//! in the crate `haex-bundle` of the vault-sdk, the one implementation the `haex` tool also uses
//! (research R3). holzi only calls it and maps its verdict to what the install dialog shows.

use serde::Serialize;
use ts_rs::TS;

use crate::error::HolziError;

pub mod manifest;
pub mod store;

pub use haex_bundle::format::limits;
pub use haex_bundle::{Migration, VerifiedBundle};
pub use manifest::{Manifest, ManifestTool, OpenerKind, ToolEffect};

/// Why a bundle was refused: the error kind of the format (`archive_invalid`, `file_mismatch`, …,
/// contracts/bundle-format.md) and, for entry-level errors and `file_mismatch`, the path inside the
/// bundle. holzi adds [`BundleRejection::FORBIDDEN_PERMISSION`] with the manifest's category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BundleRejection {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub path: Option<String>,
    /// For [`BundleRejection::FORBIDDEN_PERMISSION`]: the category as the manifest names it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub category: Option<String>,
}

impl BundleRejection {
    /// The manifest asks for a permission only the user grants when asked, never a manifest
    /// (spec 048 FR-037: access to encrypted folders).
    pub const FORBIDDEN_PERMISSION: &'static str = "forbidden_permission";

    pub fn new(kind: haex_bundle::ErrorKind) -> Self {
        Self {
            kind: kind.as_str().to_owned(),
            path: None,
            category: None,
        }
    }

    /// The refusal of a manifest that declares the forbidden `category`.
    pub fn forbidden_permission(category: &str) -> Self {
        Self {
            kind: Self::FORBIDDEN_PERMISSION.to_owned(),
            path: None,
            category: Some(category.to_owned()),
        }
    }
}

impl From<haex_bundle::BundleError> for BundleRejection {
    fn from(error: haex_bundle::BundleError) -> Self {
        // The technical message stays in the log; the dialog names the kind (and the path).
        log::info!("extension bundle refused: {error}");
        Self {
            kind: error.kind.as_str().to_owned(),
            path: error.path,
            category: None,
        }
    }
}

impl From<BundleRejection> for HolziError {
    fn from(rejection: BundleRejection) -> Self {
        HolziError::ExtensionInstall {
            reason: rejection.kind,
        }
    }
}

/// Verifies a `.xt` archive with every rule of the format.
pub fn verify_bundle(bytes: &[u8]) -> Result<VerifiedBundle, BundleRejection> {
    Ok(haex_bundle::verify_archive(bytes)?)
}

/// Lower-case hex of the SHA-256 of the signed message, the input of
/// [`crate::extensions::ids::bundle_id`].
pub fn signed_message_hex(bundle: &VerifiedBundle) -> String {
    crate::sync::keys::hex(&bundle.signed_message_sha256)
}
