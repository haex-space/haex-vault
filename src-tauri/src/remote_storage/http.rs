//! The HTTP client of the storages (spec 038 research R8, spec 050 research R2): it reaches the host
//! of an address only at the addresses [`address::pin`] checked, follows no redirect and uses no
//! proxy. Every call resolves and checks the addresses again; a client is kept per host and checked
//! addresses, so calls to the same server share its connections (and TLS sessions) while an
//! address that changes gets a client of its own.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Mutex, OnceLock};

use reqwest::Url;
use tokio::time::Instant;

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
    let mut addrs = pinned.addrs.clone();
    addrs.sort_unstable();
    // The scope too: a client of a `local` endpoint never serves the same name once it is `public`.
    let key = (scope, pinned.host.clone(), addrs);
    if let Some(client) = clients().get(&key) {
        return Ok(client.clone());
    }
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .user_agent(concat!("holzi/", env!("CARGO_PKG_VERSION")));
    if let Some(host) = &pinned.host {
        builder = builder.resolve_to_addrs(host, &pinned.addrs);
    }
    let client = builder.build().map_err(|_| StorageError::Network)?;
    let mut kept = clients();
    // Few servers in practice; a long session with changing addresses starts over.
    if kept.len() >= MAX_CLIENTS {
        kept.clear();
    }
    kept.insert(key, client.clone());
    Ok(client)
}

/// How many clients are kept at most.
const MAX_CLIENTS: usize = 64;

/// Scope, host and checked addresses. A client carries no credentials and no default headers:
/// logins go with each request, so several logins on one host share its client.
type ClientKey = (EndpointScope, Option<String>, Vec<SocketAddr>);

fn clients() -> std::sync::MutexGuard<'static, HashMap<ClientKey, reqwest::Client>> {
    static CLIENTS: OnceLock<Mutex<HashMap<ClientKey, reqwest::Client>>> = OnceLock::new();
    CLIENTS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Reads `response` up to `max` bytes before `deadline`; more is [`StorageError::TooLarge`].
pub(super) async fn read_body(
    mut response: reqwest::Response,
    max: usize,
    deadline: Instant,
) -> Result<Vec<u8>, StorageError> {
    let mut body = Vec::new();
    while let Some(chunk) = tokio::time::timeout_at(deadline, response.chunk())
        .await
        .map_err(|_| StorageError::TimedOut)?
        .map_err(|_| StorageError::Network)?
    {
        if body.len() + chunk.len() > max {
            return Err(StorageError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}
