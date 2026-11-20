//! The catalog: which CLI tools haex-vault can install into a vault and which versions of each it has
//! tested (spec 094 FR-009, FR-015, `data-model.md`).
//!
//! It is data shipped with haex-vault (`catalog.json`, written by `scripts/cli-tools/add-version.ts`
//! from signature-checked vendor data). An install only ever takes a version of this list, and
//! checks the download against the SHA-256 listed here, so the check does not depend on the
//! vendor's server at install time (spec FR-006). Adding a vendor is adding data; nothing in the
//! install, update, removal or path logic names a vendor.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use semver::Version;
use serde::Deserialize;

use super::platform::PlatformKey;

const EMBEDDED: &str = include_str!("catalog.json");

/// The only catalog format this code reads.
const SCHEMA: u32 = 1;

/// The shipped catalog was rejected: a bug in the data, caught by a test over the shipped file.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("catalog: {0}")]
pub struct CatalogError(String);

/// Every vendor haex-vault can install a tool for.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub schema: u32,
    pub vendors: Vec<VendorDefinition>,
}

/// One vendor's tool: where it comes from, how it is started and the tested versions.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VendorDefinition {
    /// The id the delegate adapter and the provider row use (`claude`, `codex`).
    pub vendor: String,
    pub display_name: String,
    /// The hosts a download URL of this vendor may name.
    pub download_hosts: Vec<String>,
    /// Per platform key, the program's path inside an installation, relative to its folder.
    pub executable: BTreeMap<String, String>,
    /// Variables always set on the tool (its own update switch).
    #[serde(default)]
    pub env_set: Vec<EnvPair>,
    /// Arguments always passed before the tool's own (its update and plugin switches).
    #[serde(default)]
    pub config_args: Vec<String>,
    /// Host variables removed before the start: the ones that decide which account or endpoint
    /// the tool talks to.
    #[serde(default)]
    pub env_remove_prefixes: Vec<String>,
    /// The variable the tool reads its configuration folder from; it points at the run folder.
    pub config_env: String,
    /// Further variables that point at the run's temp folder (a tool's own temp location).
    #[serde(default)]
    pub tmp_env: Vec<String>,
    /// The tested versions, newest first.
    pub versions: Vec<CatalogVersion>,
}

/// A variable set on the tool.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnvPair {
    pub name: String,
    pub value: String,
}

/// One tested version with its artifact per platform key.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CatalogVersion {
    pub version: String,
    pub platforms: BTreeMap<String, Artifact>,
}

/// What a download is and how it is checked.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub url: String,
    /// 64 lowercase hex characters.
    pub sha256: String,
    /// The size of the download in bytes.
    pub size: u64,
    pub kind: ArtifactKind,
    /// Bytes on disk after unpacking, for the free-space check; required for a `tar.gz`.
    #[serde(default)]
    pub unpacked_size: Option<u64>,
}

/// How a download becomes an installation.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum ArtifactKind {
    /// The program itself, placed under its executable name.
    #[serde(rename = "file")]
    File,
    /// A package, unpacked into the installation folder.
    #[serde(rename = "tar.gz")]
    TarGz,
}

impl Catalog {
    /// The catalog shipped with this build.
    ///
    /// # Panics
    ///
    /// When the embedded file is invalid. A test over the shipped file keeps that from reaching
    /// a release.
    pub fn shipped() -> &'static Catalog {
        static SHIPPED: OnceLock<Catalog> = OnceLock::new();
        SHIPPED
            .get_or_init(|| Self::parse(EMBEDDED).unwrap_or_else(|error| panic!("shipped {error}")))
    }

    /// The catalog in use: the stand-in of an e2e run (debug builds only), else the shipped one.
    pub fn active() -> &'static Catalog {
        super::stand_in::active().unwrap_or_else(Self::shipped)
    }

    /// Reads and validates a catalog; tests call it with their own JSON.
    pub fn parse(json: &str) -> Result<Catalog, CatalogError> {
        Self::parse_with(json, false)
    }

    /// As [`Catalog::parse`], but a download may come from this device over plain HTTP: for the
    /// local server of an e2e run, never for a catalog that ships.
    pub fn parse_loopback(json: &str) -> Result<Catalog, CatalogError> {
        Self::parse_with(json, true)
    }

    fn parse_with(json: &str, loopback: bool) -> Result<Catalog, CatalogError> {
        let catalog: Catalog =
            serde_json::from_str(json).map_err(|error| CatalogError(error.to_string()))?;
        catalog.validate(loopback)?;
        Ok(catalog)
    }

    /// The vendor with this id.
    pub fn vendor(&self, vendor: &str) -> Option<&VendorDefinition> {
        self.vendors.iter().find(|entry| entry.vendor == vendor)
    }

    fn validate(&self, loopback: bool) -> Result<(), CatalogError> {
        if self.schema != SCHEMA {
            return Err(fail(format!("schema {} is not {SCHEMA}", self.schema)));
        }
        for (index, vendor) in self.vendors.iter().enumerate() {
            if self.vendors[..index]
                .iter()
                .any(|earlier| earlier.vendor == vendor.vendor)
            {
                return Err(fail(format!("vendor {} is listed twice", vendor.vendor)));
            }
            vendor.validate(loopback)?;
        }
        Ok(())
    }
}

impl VendorDefinition {
    /// The artifact of `version` for `platform`, `None` when the vendor has none for it.
    pub fn artifact(&self, version: &str, platform: PlatformKey) -> Option<&Artifact> {
        self.versions
            .iter()
            .find(|entry| entry.version == version)?
            .platforms
            .get(platform.as_str())
    }

    /// The program's path inside an installation on `platform`.
    pub fn executable_for(&self, platform: PlatformKey) -> Option<&str> {
        self.executable.get(platform.as_str()).map(String::as_str)
    }

    /// The newest listed version that has an artifact for `platform`.
    pub fn newest_for(&self, platform: PlatformKey) -> Option<&CatalogVersion> {
        self.versions
            .iter()
            .find(|entry| entry.platforms.contains_key(platform.as_str()))
    }

    fn validate(&self, loopback: bool) -> Result<(), CatalogError> {
        let name = &self.vendor;
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        {
            return Err(fail(format!(
                "vendor id {name:?} is not lowercase letters and digits"
            )));
        }
        if self.versions.is_empty() {
            return Err(fail(format!("{name}: no version listed")));
        }
        if self.download_hosts.is_empty() {
            return Err(fail(format!("{name}: no download host listed")));
        }
        if self.config_env.is_empty() {
            return Err(fail(format!("{name}: config_env is empty")));
        }
        for key in self.executable.keys() {
            if PlatformKey::parse(key).is_none() {
                return Err(fail(format!(
                    "{name}: unknown platform key {key} in executable"
                )));
            }
        }
        let mut previous: Option<Version> = None;
        for entry in &self.versions {
            let version = Version::parse(&entry.version)
                .map_err(|_| fail(format!("{name}: {} is not a semver version", entry.version)))?;
            if previous.as_ref().is_some_and(|newer| *newer <= version) {
                return Err(fail(format!(
                    "{name}: versions must be strictly descending, {} breaks it",
                    entry.version
                )));
            }
            previous = Some(version);
            self.validate_version(entry, loopback)?;
        }
        Ok(())
    }

    fn validate_version(&self, entry: &CatalogVersion, loopback: bool) -> Result<(), CatalogError> {
        let name = &self.vendor;
        if entry.platforms.is_empty() {
            return Err(fail(format!(
                "{name} {}: no platform listed",
                entry.version
            )));
        }
        for (key, artifact) in &entry.platforms {
            let at = format!("{name} {} {key}", entry.version);
            if PlatformKey::parse(key).is_none() {
                return Err(fail(format!("{at}: unknown platform key")));
            }
            if !self.executable.contains_key(key) {
                return Err(fail(format!("{at}: no executable path for this platform")));
            }
            artifact.validate(&at, &self.download_hosts, loopback)?;
        }
        Ok(())
    }
}

impl Artifact {
    fn validate(&self, at: &str, hosts: &[String], loopback: bool) -> Result<(), CatalogError> {
        let url =
            url::Url::parse(&self.url).map_err(|_| fail(format!("{at}: url is not a URL")))?;
        let host = url.host_str().unwrap_or_default();
        let local = loopback && url.scheme() == "http" && matches!(host, "127.0.0.1" | "localhost");
        if url.scheme() != "https" && !local {
            return Err(fail(format!("{at}: url is not https")));
        }
        if !local && !hosts.iter().any(|allowed| allowed == host) {
            return Err(fail(format!(
                "{at}: host {host} is not a download host of the vendor"
            )));
        }
        let hex = self.sha256.len() == 64
            && self
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !hex {
            return Err(fail(format!(
                "{at}: sha256 is not 64 lowercase hex characters"
            )));
        }
        if self.size == 0 {
            return Err(fail(format!("{at}: size is 0")));
        }
        match (self.kind, self.unpacked_size) {
            (ArtifactKind::TarGz, None) => Err(fail(format!("{at}: a tar.gz needs unpacked_size"))),
            (ArtifactKind::File, Some(_)) => {
                Err(fail(format!("{at}: a file has no unpacked_size")))
            }
            _ => Ok(()),
        }
    }
}

fn fail(message: String) -> CatalogError {
    CatalogError(message)
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
