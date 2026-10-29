//! The icons of the catalog (spec 069, FR-006, research R12): the webview never calls an address
//! of a marketplace, the host loads the icon and sends a `data:` URL. An icon address comes from a
//! marketplace and is therefore untrusted: it must lead to a publicly routable address (a hostile
//! marketplace must not make the host call the local network), only a PNG, JPEG or WebP is
//! accepted by its bytes (never an SVG), and no login ever goes with the request.
//!
//! The resolved address is checked and then pinned for the connection, so a changing DNS answer
//! cannot slip past the check, and every redirect is checked again.

use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use futures::future::BoxFuture;
use futures::StreamExt;
use reqwest::header::LOCATION;
use reqwest::redirect::Policy;
use tokio::sync::Semaphore;
use url::{Origin, Url};

use super::{app_version, ICON_CACHE_ENTRIES, ICON_MAX_BYTES, ICON_PARALLEL, SOURCE_TIMEOUT};
use crate::error::HolziError;

/// How many redirects an icon request follows.
const MAX_REDIRECTS: usize = 5;

/// Whether `ip` is an address of the public internet: not this host, not a local network, not a
/// reserved or special-purpose range.
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => is_public_v6(v6),
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0                                   // "this" network
        || a == 10                             // private
        || a == 127                            // loopback
        || (a == 100 && (64..=127).contains(&b)) // shared address space (CGNAT)
        || (a == 169 && b == 254)              // link-local
        || (a == 172 && (16..=31).contains(&b)) // private
        || (a == 192 && b == 0 && c == 0)      // IETF protocol assignments
        || (a == 192 && b == 0 && c == 2)      // documentation
        || (a == 192 && b == 168)              // private
        || (a == 198 && (b == 18 || b == 19))  // benchmarking
        || (a == 198 && b == 51 && c == 100)   // documentation
        || (a == 203 && b == 0 && c == 113)    // documentation
        || a >= 224) // multicast, reserved, broadcast
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_v4(mapped);
    }
    let segments = ip.segments();
    // Only global unicast (2000::/3) is public; that leaves out ::, ::1, fe80::/10, fc00::/7,
    // ff00::/8 and everything else that is not routable on the internet.
    if segments[0] & 0xe000 != 0x2000 {
        return false;
    }
    // Documentation (2001:db8::/32) and 6to4 (2002::/16, carries an IPv4 address) and NAT64.
    if segments[0] == 0x2001 && segments[1] == 0x0db8 {
        return false;
    }
    if segments[0] == 0x2002 {
        let embedded = Ipv4Addr::new(
            (segments[1] >> 8) as u8,
            segments[1] as u8,
            (segments[2] >> 8) as u8,
            segments[2] as u8,
        );
        return is_public_v4(embedded);
    }
    true
}

/// How the host finds an address and connects to it. The default asks the system's resolver and
/// connects to what was resolved; the tests stand in for both.
#[derive(Clone)]
pub struct Network {
    resolve: Arc<dyn Fn(String, u16) -> BoxFuture<'static, Vec<IpAddr>> + Send + Sync>,
    connect: Arc<dyn Fn(IpAddr, u16) -> SocketAddr + Send + Sync>,
}

impl Network {
    /// The system's resolver, connecting to the resolved address.
    pub fn system() -> Self {
        Self {
            resolve: Arc::new(|host, port| {
                Box::pin(async move {
                    tokio::net::lookup_host((host.as_str(), port))
                        .await
                        .map(|addresses| addresses.map(|a| a.ip()).collect())
                        .unwrap_or_default()
                })
            }),
            connect: Arc::new(SocketAddr::new),
        }
    }

    /// A network with its own resolver and connector (tests).
    pub fn with(
        resolve: impl Fn(String, u16) -> BoxFuture<'static, Vec<IpAddr>> + Send + Sync + 'static,
        connect: impl Fn(IpAddr, u16) -> SocketAddr + Send + Sync + 'static,
    ) -> Self {
        Self {
            resolve: Arc::new(resolve),
            connect: Arc::new(connect),
        }
    }
}

/// Why an icon is not loaded; the caller only ever says "unavailable".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconRefusal {
    /// The address is not `http(s)`, has a login in it, or an `http` address on another origin.
    Address,
    /// The address leads somewhere that is not publicly routable.
    Destination,
    /// The request failed or the answer was no success.
    Fetch,
    /// More than [`ICON_MAX_BYTES`].
    TooLarge,
    /// Not a PNG, JPEG or WebP by its bytes.
    NotAnImage,
}

/// The scheme rules for one hop: `http` or `https`, no login in the address, and `https` unless the
/// address is on the origin of the marketplace itself.
pub fn check_address(source: &Origin, target: &Url) -> Result<(), IconRefusal> {
    if !matches!(target.scheme(), "http" | "https") || target.host_str().is_none() {
        return Err(IconRefusal::Address);
    }
    if !target.username().is_empty() || target.password().is_some() {
        return Err(IconRefusal::Address);
    }
    if target.scheme() == "http" && target.origin() != *source {
        return Err(IconRefusal::Address);
    }
    Ok(())
}

/// The kind of image by its first bytes: PNG, JPEG or WebP. An SVG, HTML or anything else is none,
/// whatever its `Content-Type` claims.
pub fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

struct Cache {
    entries: VecDeque<(String, String)>,
}

/// Loads icons: with a cache of the last [`ICON_CACHE_ENTRIES`], at most [`ICON_PARALLEL`] loads at
/// once.
pub struct Icons {
    cache: Mutex<Cache>,
    limit: Arc<Semaphore>,
    capacity: usize,
}

impl Default for Icons {
    fn default() -> Self {
        Self::new(ICON_PARALLEL, ICON_CACHE_ENTRIES)
    }
}

impl Icons {
    pub fn new(parallel: usize, capacity: usize) -> Self {
        Self {
            cache: Mutex::new(Cache {
                entries: VecDeque::new(),
            }),
            limit: Arc::new(Semaphore::new(parallel)),
            capacity,
        }
    }

    /// The semaphore of the parallel loads (the tests hold permits to see a load wait).
    pub fn limit(&self) -> Arc<Semaphore> {
        Arc::clone(&self.limit)
    }

    fn cached(&self, key: &str) -> Option<String> {
        let cache = self.cache.lock().ok()?;
        cache
            .entries
            .iter()
            .find(|(known, _)| known == key)
            .map(|(_, data)| data.clone())
    }

    fn remember(&self, key: String, data: String) {
        if let Ok(mut cache) = self.cache.lock() {
            if cache.entries.iter().any(|(known, _)| *known == key) {
                return;
            }
            while cache.entries.len() >= self.capacity.max(1) {
                cache.entries.pop_front();
            }
            cache.entries.push_back((key, data));
        }
    }

    /// The icon at `url` as a `data:` URL, for a marketplace at `source_base`.
    pub async fn load(
        &self,
        network: &Network,
        source_base: &str,
        url: &str,
    ) -> Result<String, HolziError> {
        if let Some(data) = self.cached(url) {
            return Ok(data);
        }
        let Ok(_permit) = self.limit.acquire().await else {
            return Err(unavailable());
        };
        let result = fetch(network, source_base, url).await;
        match result {
            Ok((mime, bytes)) => {
                let data = format!(
                    "data:{mime};base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                );
                self.remember(url.to_owned(), data.clone());
                Ok(data)
            }
            Err(refusal) => {
                log::debug!("an icon is not loaded: {refusal:?}");
                Err(unavailable())
            }
        }
    }
}

fn unavailable() -> HolziError {
    HolziError::Marketplace {
        reason: super::kinds::ICON_UNAVAILABLE.to_owned(),
    }
}

/// Follows the address (and its redirects) and returns the kind and the bytes of the image.
pub async fn fetch(
    network: &Network,
    source_base: &str,
    url: &str,
) -> Result<(&'static str, Vec<u8>), IconRefusal> {
    let source = Url::parse(source_base)
        .map_err(|_| IconRefusal::Address)?
        .origin();
    let mut current = Url::parse(url).map_err(|_| IconRefusal::Address)?;
    for _ in 0..=MAX_REDIRECTS {
        check_address(&source, &current)?;
        let host = current.host_str().ok_or(IconRefusal::Address)?.to_owned();
        let port = current
            .port_or_known_default()
            .ok_or(IconRefusal::Address)?;
        let addresses = match current.host() {
            Some(url::Host::Ipv4(ip)) => vec![IpAddr::V4(ip)],
            Some(url::Host::Ipv6(ip)) => vec![IpAddr::V6(ip)],
            _ => (network.resolve)(host.clone(), port).await,
        };
        // One non-public address among the answers is enough to refuse: the connection may pick
        // any of them.
        if addresses.is_empty() || !addresses.iter().all(|ip| is_public_ip(*ip)) {
            return Err(IconRefusal::Destination);
        }
        let pinned = (network.connect)(addresses[0], port);
        let mut builder = reqwest::Client::builder()
            .user_agent(format!("HaexVault/{}", app_version()))
            .redirect(Policy::none())
            .no_proxy()
            .timeout(SOURCE_TIMEOUT);
        if current
            .host()
            .is_some_and(|h| matches!(h, url::Host::Domain(_)))
        {
            builder = builder.resolve(&host, pinned);
        }
        let client = builder.build().map_err(|_| IconRefusal::Fetch)?;
        let response = client
            .get(current.clone())
            .send()
            .await
            .map_err(|_| IconRefusal::Fetch)?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or(IconRefusal::Fetch)?;
            let next = current.join(location).map_err(|_| IconRefusal::Address)?;
            if current.scheme() == "https" && next.scheme() == "http" {
                return Err(IconRefusal::Address);
            }
            current = next;
            continue;
        }
        if !response.status().is_success() {
            return Err(IconRefusal::Fetch);
        }
        let mut stream = response.bytes_stream();
        let mut body = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| IconRefusal::Fetch)?;
            if body.len() + chunk.len() > ICON_MAX_BYTES {
                return Err(IconRefusal::TooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        let mime = image_mime(&body).ok_or(IconRefusal::NotAnImage)?;
        return Ok((mime, body));
    }
    Err(IconRefusal::Fetch)
}

#[cfg(test)]
#[path = "icons_tests.rs"]
mod tests;
