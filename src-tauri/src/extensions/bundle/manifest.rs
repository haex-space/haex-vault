//! The typed view of a verified manifest (data-model.md §Rust-Typen).
//!
//! The manifest is valid once `haex-bundle` accepted the bundle; this view only reads the fields
//! holzi uses. Optional fields of the wrong type count as absent, never as an error: the format
//! does not constrain them, so the tool would sign them.

use std::collections::BTreeMap;

use haex_bundle::jcs::JsonValue;

use super::{BundleRejection, VerifiedBundle};
use crate::extensions::ids::{ExtensionName, PublicKey};
use crate::extensions::permissions::manifest_map::{map_manifest_permissions, DeclaredPermissions};

const DEFAULT_ENTRY: &str = "index.html";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub name: ExtensionName,
    pub version: semver::Version,
    pub public_key: PublicKey,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub homepage: Option<String>,
    /// Path of the icon inside the bundle.
    pub icon: Option<String>,
    /// Path of the entry page inside the bundle.
    pub entry: String,
    pub single_instance: bool,
    pub migrations_dir: Option<String>,
    /// Translations of name and description per locale, as the manifest writes them.
    pub i18n: Option<serde_json::Value>,
    pub permissions: DeclaredPermissions,
}

fn text(manifest: &BTreeMap<String, JsonValue>, key: &str) -> Option<String> {
    manifest
        .get(key)
        .and_then(JsonValue::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

impl Manifest {
    /// Reads the manifest of a bundle that `haex-bundle` verified. `displayMode` is accepted and
    /// ignored: an extension always opens as a tab (FR-014).
    pub fn from_verified(bundle: &VerifiedBundle) -> Result<Self, BundleRejection> {
        Self::from_object(&bundle.manifest)?.allowed()
    }

    /// Refuses a manifest that declares a permission no manifest may carry (spec 048 FR-037), at
    /// install, update and when a development version loads. A stored manifest is read without
    /// this check: it was checked when it was installed.
    fn allowed(self) -> Result<Self, BundleRejection> {
        match self.permissions.forbidden_categories.first() {
            Some(category) => Err(BundleRejection::forbidden_permission(category)),
            None => Ok(self),
        }
    }

    /// Reads `haextension/manifest.json` of a project loaded in developer mode (US12): restricted
    /// JSON, not necessarily canonical, and unsigned; the same fields and rules otherwise.
    ///
    /// The project's `package.json` fills in as the SDK's `readManifest` does when it builds the
    /// extension: its `name` always wins (the SDK names the tables with it, and `haex init` writes
    /// no name into the manifest), `version`, `author` and `homepage` count where the manifest has
    /// none. A `package.json` that is missing or no JSON object adds nothing, as in the SDK.
    pub fn from_dev_file(
        manifest_json: &str,
        package_json: Option<&str>,
    ) -> Result<Self, BundleRejection> {
        let Ok(JsonValue::Object(mut manifest)) = haex_bundle::jcs::parse_restricted(manifest_json)
        else {
            return Err(BundleRejection::new(
                haex_bundle::ErrorKind::ManifestInvalid,
            ));
        };
        let package = package_json
            .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
            .unwrap_or_default();
        let from_package = |key: &str| {
            package
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(|value| JsonValue::String(value.to_owned()))
        };
        if let Some(name) = from_package("name") {
            manifest.insert("name".to_owned(), name);
        }
        for key in ["version", "author", "homepage"] {
            if matches!(manifest.get(key), None | Some(JsonValue::Null)) {
                if let Some(value) = from_package(key) {
                    manifest.insert(key.to_owned(), value);
                }
            }
        }
        Self::from_object(&manifest)?.allowed()
    }

    /// Reads the stored `manifest_json` of a bundle that was verified when it was installed.
    pub fn from_stored(manifest_json: &[u8]) -> Result<Self, BundleRejection> {
        match haex_bundle::jcs::parse_canonical(manifest_json) {
            Ok(JsonValue::Object(manifest)) => Self::from_object(&manifest),
            _ => Err(BundleRejection::new(
                haex_bundle::ErrorKind::ManifestNotCanonical,
            )),
        }
    }

    fn from_object(manifest: &BTreeMap<String, JsonValue>) -> Result<Self, BundleRejection> {
        let invalid = || BundleRejection::new(haex_bundle::ErrorKind::ManifestInvalid);
        let name = text(manifest, "name")
            .and_then(|n| ExtensionName::parse(&n).ok())
            .ok_or_else(invalid)?;
        let version = text(manifest, "version")
            .and_then(|v| semver::Version::parse(&v).ok())
            .ok_or_else(invalid)?;
        let public_key = text(manifest, "publicKey")
            .and_then(|k| PublicKey::parse(&k).ok())
            .ok_or_else(invalid)?;
        let permissions = manifest
            .get("permissions")
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::Null);
        Ok(Self {
            name,
            version,
            public_key,
            display_name: text(manifest, "displayName"),
            description: text(manifest, "description"),
            author: text(manifest, "author"),
            homepage: text(manifest, "homepage"),
            icon: text(manifest, "icon"),
            entry: text(manifest, "entry").unwrap_or_else(|| DEFAULT_ENTRY.to_owned()),
            single_instance: matches!(manifest.get("singleInstance"), Some(JsonValue::Bool(true))),
            migrations_dir: text(manifest, "migrationsDir"),
            i18n: manifest
                .get("i18n")
                .filter(|v| v.as_object().is_some())
                .map(serde_json::Value::from),
            permissions: map_manifest_permissions(&permissions),
        })
    }

    /// The name to show: `displayName`, else `name`.
    pub fn title(&self) -> &str {
        self.display_name
            .as_deref()
            .unwrap_or_else(|| self.name.as_str())
    }
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod manifest_tests;
