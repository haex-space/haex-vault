//! The marketplace for extensions (spec 069): the host asks the marketplaces of a vault for
//! extensions, checks what it downloads and hands the bytes to the install preview of spec 017.
//!
//! Every marketplace is an equal row of `haex_marketplaces` (name, address, optional login); the
//! public one is only the preset row of [`store`]. The protocol they speak is the contract in
//! `specs/069-marketplace/contracts/marketplace-api.md`; the host knows nothing else about them.

use std::time::Duration;

pub mod catalog;
pub mod client;
pub mod commands;
pub mod details;
pub mod download;
pub mod icons;
pub mod model;
pub mod pending;
pub mod seed;
pub mod store;
pub mod wire;

#[cfg(test)]
mod test_support;

/// How long one marketplace may take to answer one call of the catalog, the details or the test of
/// the form (research R5). A slower one counts as down and holds up none of the others.
pub const SOURCE_TIMEOUT: Duration = Duration::from_secs(8);

/// How long connecting may take.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a download may stay silent between two chunks. There is no total limit: bundles are
/// large and networks slow (research R5).
pub const DOWNLOAD_IDLE_TIMEOUT: Duration = Duration::from_secs(20);

/// Entries per page and source (contract: HaexVault always asks for 20).
pub const PAGE_LIMIT: u32 = 20;

/// How long a downloaded bundle waits for its install preview before it is dropped.
pub const PENDING_IDLE: Duration = Duration::from_secs(10 * 60);

/// The most bytes of an icon the host loads (research R12).
pub const ICON_MAX_BYTES: usize = 256 * 1024;

/// How many icons the host keeps in memory.
pub const ICON_CACHE_ENTRIES: usize = 200;

/// How many icons load at the same time.
pub const ICON_PARALLEL: usize = 4;

/// The shortest time between two progress reports of a download.
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// The error kinds of `HolziError::Marketplace` (contracts/commands.md "Error kinds"); the one
/// place of their spelling.
pub mod kinds {
    pub const INVALID_URL: &str = "marketplace_invalid_url";
    pub const CREDENTIALS_REQUIRED: &str = "marketplace_credentials_required";
    pub const INSECURE_CREDENTIALS: &str = "marketplace_insecure_credentials";
    pub const NOT_FOUND: &str = "marketplace_not_found";
    pub const NO_HASH: &str = "marketplace_no_hash";
    pub const HASH_MISMATCH: &str = "marketplace_hash_mismatch";
    pub const SIZE_MISMATCH: &str = "marketplace_size_mismatch";
    pub const TOO_LARGE: &str = "marketplace_too_large";
    pub const INCOMPATIBLE_VERSION: &str = "marketplace_incompatible_version";
    pub const DOWNLOAD_CANCELLED: &str = "marketplace_download_cancelled";
    pub const ICON_UNAVAILABLE: &str = "marketplace_icon_unavailable";
    pub const PENDING_MISSING: &str = "marketplace_pending_missing";
    pub const DESTINATION_REFUSED: &str = "marketplace_destination_refused";
    pub const SOURCE_UNREACHABLE: &str = "marketplace_source_unreachable";
    pub const SOURCE_TIMEOUT: &str = "marketplace_source_timeout";
    pub const SOURCE_AUTH_REJECTED: &str = "marketplace_source_auth_rejected";
    pub const SOURCE_BAD_RESPONSE: &str = "marketplace_source_bad_response";
    pub const CREDENTIALS_SYNCING: &str = "marketplace_credentials_syncing";
    /// A newer `marketplace_catalog` call took the place of this one.
    pub const SUPERSEDED: &str = "marketplace_superseded";
}

/// What the marketplace commands keep between two calls.
pub struct MarketplaceState {
    /// The catalog call that is running; a newer call aborts it (contracts/commands.md).
    catalog: std::sync::Mutex<Option<futures::future::AbortHandle>>,
    pub icons: icons::Icons,
    /// The downloaded bundle that waits for its install preview (one at a time).
    pub pending: std::sync::Arc<pending::PendingSlot>,
    /// The downloads that are running, so one can be cancelled by its id.
    pub downloads: download::Downloads,
}

impl Default for MarketplaceState {
    fn default() -> Self {
        Self {
            catalog: std::sync::Mutex::new(None),
            icons: icons::Icons::default(),
            pending: std::sync::Arc::default(),
            downloads: download::Downloads::default(),
        }
    }
}

impl MarketplaceState {
    /// Registers a new catalog call and aborts the one still running.
    pub fn begin_catalog(&self) -> futures::future::AbortRegistration {
        let (handle, registration) = futures::future::AbortHandle::new_pair();
        if let Ok(mut running) = self.catalog.lock() {
            if let Some(previous) = running.replace(handle) {
                previous.abort();
            }
        }
        registration
    }
}

/// The name the marketplaces see for this platform in `X-Platform`: `linux`, `windows`, `macos`,
/// `android` or `ios`.
pub fn platform() -> &'static str {
    platform_name(std::env::consts::OS)
}

/// [`platform`] for a given `std::env::consts::OS`; anything unknown is `linux`.
pub fn platform_name(os: &str) -> &'static str {
    match os {
        "windows" => "windows",
        "macos" => "macos",
        "android" => "android",
        "ios" => "ios",
        _ => "linux",
    }
}

/// The version of HaexVault as the marketplaces see it in `X-App-Version` and as `minAppVersion`
/// and `maxAppVersion` are compared against.
pub fn app_version() -> semver::Version {
    semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version is semver")
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
