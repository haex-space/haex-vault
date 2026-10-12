//! The HTTP client of the storages (spec 038 research R8, spec 050 research R2): it reaches the host
//! of an address only at the addresses [`address::pin`] checked, follows no redirect and uses no
//! proxy. S3 and WebDAV build one per call.

use reqwest::Url;

use super::address::{self, Resolver};
use super::{EndpointScope, StorageError};

/// A client that reaches the host of `url` only at its checked addresses; a refused address is
/// [`StorageError::Network`].
pub(super) async fn pinned_client(
    url: &Url,
    scope: EndpointScope,
    resolver: &dyn Resolver,
) -> Result<reqwest::Client, StorageError> {
    let pinned = address::pin(url, scope, resolver).await.map_err(|error| {
        log::warn!("remote storage: endpoint refused: {error}");
        StorageError::Network
    })?;
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .user_agent(concat!("holzi/", env!("CARGO_PKG_VERSION")));
    if let Some(host) = &pinned.host {
        builder = builder.resolve_to_addrs(host, &pinned.addrs);
    }
    builder.build().map_err(|_| StorageError::Network)
}
