//! Storage-layer provider identity — single source of truth.
//!
//! [`ProviderKind`] is the canonical enum used by every layer that names an
//! S3-compatible provider (persisted IAM-admin creds, share-command args
//! from the frontend, the adapter-flavor conversion). Consumers stop
//! validating provider strings pairwise: unknown values fail at the serde
//! boundary, and the compiler enforces exhaustiveness.
//!
//! The `Minio` variant is accepted by the enum but rejected by
//! [`ProviderKind::to_flavor`] — MinIO's admin API is JSON-shaped and lives
//! on a separate adapter (deferred task).

use serde::{Deserialize, Serialize};

use crate::remote_storage::iam_adapter::ProviderFlavor;

/// Storage-layer provider identity.
///
/// Serialised as lowercase (`"aws" | "wasabi" | "minio"`) both by serde (for
/// wire-format on Tauri command args) and by the `to_slug()` helper (for
/// password-manager `haex_passwords_item_key_values.value` storage).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Aws,
    Wasabi,
    Minio,
}

impl ProviderKind {
    /// Lowercase slug for password-manager string storage. Matches serde's
    /// wire form — kept as a small helper to sidestep serde ceremony when
    /// we just need `&str`.
    pub fn to_slug(self) -> &'static str {
        match self {
            ProviderKind::Aws => "aws",
            ProviderKind::Wasabi => "wasabi",
            ProviderKind::Minio => "minio",
        }
    }

    /// Parse a stored slug back into the enum. Returns `None` on unknown
    /// values so callers (e.g. `iam_admin_creds::load`) can treat data
    /// corruption / removed variants as "broken entry" rather than
    /// panicking.
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "aws" => Some(ProviderKind::Aws),
            "wasabi" => Some(ProviderKind::Wasabi),
            "minio" => Some(ProviderKind::Minio),
            _ => None,
        }
    }

    /// Convert to the runtime adapter shape. `Minio` is rejected — the
    /// AWS-compatible adapter cannot drive MinIO's JSON admin API.
    pub fn to_flavor(self) -> Result<ProviderFlavor, ProviderError> {
        match self {
            ProviderKind::Aws => Ok(ProviderFlavor::Aws),
            ProviderKind::Wasabi => Ok(ProviderFlavor::Wasabi),
            ProviderKind::Minio => Err(ProviderError::MinioNotYetImplemented),
        }
    }
}

/// Errors surfaced when converting [`ProviderKind`] into a runtime adapter
/// shape. Kept structural — the frontend surfaces the message to the user.
#[derive(thiserror::Error, Debug)]
pub enum ProviderError {
    #[error("MinIO adapter not yet implemented")]
    MinioNotYetImplemented,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kind_serializes_to_lowercase_string() {
        assert_eq!(
            serde_json::to_string(&ProviderKind::Aws).unwrap(),
            "\"aws\""
        );
        assert_eq!(
            serde_json::to_string(&ProviderKind::Wasabi).unwrap(),
            "\"wasabi\""
        );
        assert_eq!(
            serde_json::to_string(&ProviderKind::Minio).unwrap(),
            "\"minio\""
        );
    }

    #[test]
    fn provider_kind_deserializes_from_lowercase_string() {
        let aws: ProviderKind = serde_json::from_str("\"aws\"").unwrap();
        assert_eq!(aws, ProviderKind::Aws);
        let wasabi: ProviderKind = serde_json::from_str("\"wasabi\"").unwrap();
        assert_eq!(wasabi, ProviderKind::Wasabi);
        let minio: ProviderKind = serde_json::from_str("\"minio\"").unwrap();
        assert_eq!(minio, ProviderKind::Minio);
    }

    #[test]
    fn provider_kind_rejects_unknown_value_at_deserialization() {
        let res: Result<ProviderKind, _> = serde_json::from_str("\"gcs\"");
        assert!(res.is_err(), "unknown provider must fail to deserialize");
    }

    #[test]
    fn provider_kind_slug_round_trip() {
        for kind in [ProviderKind::Aws, ProviderKind::Wasabi, ProviderKind::Minio] {
            assert_eq!(ProviderKind::from_slug(kind.to_slug()), Some(kind));
        }
        assert_eq!(ProviderKind::from_slug("gcs"), None);
    }

    #[test]
    fn provider_kind_minio_to_flavor_yields_specific_error() {
        let err = ProviderKind::Minio
            .to_flavor()
            .expect_err("MinIO must not produce a runtime flavor");
        match err {
            ProviderError::MinioNotYetImplemented => {}
        }
        // Message must clearly mention MinIO — frontend surfaces it verbatim.
        let msg = format!("{err}");
        assert!(
            msg.to_lowercase().contains("minio"),
            "error message should mention MinIO, got: {msg}"
        );
    }

    #[test]
    fn provider_kind_aws_and_wasabi_convert_to_flavor() {
        matches!(ProviderKind::Aws.to_flavor(), Ok(ProviderFlavor::Aws));
        matches!(ProviderKind::Wasabi.to_flavor(), Ok(ProviderFlavor::Wasabi));
    }
}
