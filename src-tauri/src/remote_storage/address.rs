//! Which endpoints holzi talks to (spec 038 FR-017, FR-009b, research R8).
//!
//! [`check_endpoint`] looks at the address as typed: only `http`, `https` and `sftp` (spec 050,
//! port 22 unless given), no user name or password in it, and no IP address that is never
//! reached. [`scope_of`] fixes the scope of an
//! endpoint when it is set ([`EndpointScope`]): `local` when its host resolves to loopback or
//! private addresses only, `public` when it resolves to public addresses only. [`pin`] runs before
//! every call: it resolves the host once, checks that every address lies in the stored scope, and
//! hands back the addresses the request must go to, so a name that points elsewhere a moment later
//! (DNS rebinding) reaches nothing new.
//!
//! - Never: link-local (`169.254.0.0/16` with the metadata service of a cloud, `fe80::/10`),
//!   unspecified, multicast and broadcast addresses, also as IPv4-mapped IPv6.
//! - Loopback and private addresses (RFC 1918, RFC 4193) only for scope `local`, public addresses
//!   only for scope `public`; a host with both is refused.
//! - `http` only for scope `local`; `sftp` is encrypted like `https`.

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use async_trait::async_trait;
use reqwest::Url;
use url::Host;

use super::{EndpointScope, Location, ProviderKind};

/// Why an endpoint is refused; no variant carries the address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AddressError {
    /// Not an `http`/`https` address with a host, or it carries a user name, a password, a query
    /// or a fragment.
    #[error("invalid endpoint")]
    Invalid,
    /// The address is one holzi does not reach for this endpoint.
    #[error("endpoint not allowed")]
    NotAllowed,
    /// The host name did not resolve to any address.
    #[error("endpoint not resolved")]
    Unresolved,
}

/// What an address is, for the rules above.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Forbidden,
    Local,
    Public,
}

/// The class of `ip`; an IPv4-mapped or NAT64 IPv6 address counts as its IPv4 address.
pub fn classify(ip: IpAddr) -> Class {
    match ip {
        IpAddr::V4(v4) => classify_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped().or_else(|| nat64(v6)) {
            Some(v4) => classify_v4(v4),
            None => classify_v6(v6),
        },
    }
}

/// The IPv4 address behind the well-known NAT64 prefix `64:ff9b::/96` (RFC 6052), which a NAT64
/// gateway forwards to that IPv4 address.
fn nat64(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let s = ip.segments();
    (s[..6] == [0x64, 0xff9b, 0, 0, 0, 0]).then(|| {
        let [.., a, b, c, d] = ip.octets();
        Ipv4Addr::new(a, b, c, d)
    })
}

/// Classifies IPv4 addresses as forbidden, local or public for endpoint validation.
fn classify_v4(ip: Ipv4Addr) -> Class {
    if ip.is_link_local()
        || ip.is_unspecified()
        || ip.octets()[0] == 0
        || ip.is_multicast()
        || ip.is_broadcast()
    {
        Class::Forbidden
    } else if ip.is_loopback() || ip.is_private() {
        Class::Local
    } else {
        Class::Public
    }
}

/// Classifies an IPv6 address after the caller has handled IPv4-mapped and NAT64 addresses.
fn classify_v6(ip: Ipv6Addr) -> Class {
    let first = ip.segments()[0];
    if ip.is_unspecified() || ip.is_multicast() || first & 0xffc0 == 0xfe80 {
        Class::Forbidden
    } else if ip.is_loopback() || first & 0xfe00 == 0xfc00 {
        Class::Local
    } else {
        Class::Public
    }
}

/// Whether `ip` lies in `scope`; a forbidden address lies in none.
pub fn in_scope(ip: IpAddr, scope: EndpointScope) -> bool {
    matches!(
        (classify(ip), scope),
        (Class::Local, EndpointScope::Local) | (Class::Public, EndpointScope::Public)
    )
}

/// Whether the scheme of `url` encrypts (`https`, `sftp`); `http` does not, others are refused.
fn is_https(url: &Url) -> Result<bool, AddressError> {
    match url.scheme() {
        "https" | "sftp" => Ok(true),
        "http" => Ok(false),
        _ => Err(AddressError::Invalid),
    }
}

/// The host of `url` as a name or an IP address. The URL parser keeps the host of a scheme it
/// does not know (`sftp`) as text, so an IP address there is read here; otherwise it would pass
/// the address checks as a name.
pub fn host_of(url: &Url) -> Option<Host<String>> {
    let host = url.host()?.to_owned();
    let Host::Domain(name) = &host else {
        return Some(host);
    };
    let bare = name.trim_start_matches('[').trim_end_matches(']');
    Some(match bare.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => Host::Ipv4(ip),
        Ok(IpAddr::V6(ip)) => Host::Ipv6(ip),
        Err(_) => host,
    })
}

/// The port of `url`: the one given, else that of its scheme (22 for `sftp`).
pub fn port_of(url: &Url) -> Option<u16> {
    url.port_or_known_default()
        .or_else(|| (url.scheme() == "sftp").then_some(22))
}

/// Checks an endpoint as typed (see the module). An IP address is refused here when it is never
/// reached or when `http` would go to a public one; a host name's addresses are checked by
/// [`scope_of`] and [`pin`].
pub fn check_endpoint(endpoint: &str) -> Result<Url, AddressError> {
    let url = Url::parse(endpoint.trim()).map_err(|_| AddressError::Invalid)?;
    let https = is_https(&url)?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AddressError::Invalid);
    }
    let ip = match host_of(&url).ok_or(AddressError::Invalid)? {
        Host::Domain(_) => return Ok(url),
        Host::Ipv4(ip) => IpAddr::V4(ip),
        Host::Ipv6(ip) => IpAddr::V6(ip),
    };
    match classify(ip) {
        Class::Forbidden => Err(AddressError::NotAllowed),
        Class::Public if !https => Err(AddressError::NotAllowed),
        Class::Local | Class::Public => Ok(url),
    }
}

/// The address a connection talks to: its endpoint, or for AWS without one the regional one.
pub fn endpoint_url(location: &Location) -> Result<Url, AddressError> {
    if location.provider_kind == ProviderKind::Aws && location.endpoint.trim().is_empty() {
        return aws_endpoint(&location.region);
    }
    check_endpoint(&location.endpoint)
}

/// The regional address of AWS S3 for `region` (lower-case letters, digits and `-`).
pub fn aws_endpoint(region: &str) -> Result<Url, AddressError> {
    if region.is_empty()
        || !region
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(AddressError::Invalid);
    }
    Url::parse(&format!("https://s3.{region}.amazonaws.com")).map_err(|_| AddressError::Invalid)
}

/// Whether the endpoint sends without encryption; the settings and the dialog mark it.
pub fn is_insecure(url: &Url) -> bool {
    url.scheme() == "http"
}

/// Turns a host name into addresses; the system resolver in holzi, a fixed table in tests.
#[async_trait]
pub trait Resolver: Send + Sync {
    /// Resolves `host` and `port` to candidate IP addresses, or returns a lookup error.
    async fn lookup(&self, host: &str, port: u16) -> io::Result<Vec<IpAddr>>;
}

/// The resolver of the operating system.
pub struct SystemResolver;

#[async_trait]
impl Resolver for SystemResolver {
    /// Returns the IP addresses found by the operating system for `host` and `port`.
    async fn lookup(&self, host: &str, port: u16) -> io::Result<Vec<IpAddr>> {
        Ok(tokio::net::lookup_host((host, port))
            .await?
            .map(|addr| addr.ip())
            .collect())
    }
}

/// The checked addresses of one request's host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pinned {
    /// The host name the request keeps for TLS and the signature; `None` for an IP address.
    pub host: Option<String>,
    pub addrs: Vec<SocketAddr>,
}

/// The host of `url` (`None` for an IP address), its port and its addresses, looked up once.
async fn addresses(
    url: &Url,
    resolver: &dyn Resolver,
) -> Result<(Option<String>, u16, Vec<IpAddr>), AddressError> {
    let port = port_of(url).ok_or(AddressError::Invalid)?;
    let (host, ips) = match host_of(url).ok_or(AddressError::Invalid)? {
        Host::Ipv4(ip) => (None, vec![IpAddr::V4(ip)]),
        Host::Ipv6(ip) => (None, vec![IpAddr::V6(ip)]),
        Host::Domain(name) => {
            let ips = resolver
                .lookup(&name, port)
                .await
                .map_err(|_| AddressError::Unresolved)?;
            (Some(name), ips)
        }
    };
    if ips.is_empty() {
        return Err(AddressError::Unresolved);
    }
    Ok((host, port, ips))
}

/// The scope of the endpoint `url` when it is set (see the module): `local` when every address is
/// loopback or private, `public` when every address is public. A forbidden address, a mix of both
/// and `http` to a public host are refused.
pub async fn scope_of(url: &Url, resolver: &dyn Resolver) -> Result<EndpointScope, AddressError> {
    let https = is_https(url)?;
    let (_, _, ips) = addresses(url, resolver).await?;
    let scope = if ips.iter().all(|ip| classify(*ip) == Class::Local) {
        EndpointScope::Local
    } else if https && ips.iter().all(|ip| classify(*ip) == Class::Public) {
        EndpointScope::Public
    } else {
        return Err(AddressError::NotAllowed);
    };
    Ok(scope)
}

/// Resolves the host of `url` once and checks that every address lies in `scope` (see the
/// module). One address outside it refuses the whole request.
pub async fn pin(
    url: &Url,
    scope: EndpointScope,
    resolver: &dyn Resolver,
) -> Result<Pinned, AddressError> {
    if !is_https(url)? && scope != EndpointScope::Local {
        return Err(AddressError::NotAllowed);
    }
    let (host, port, ips) = addresses(url, resolver).await?;
    if !ips.iter().all(|ip| in_scope(*ip, scope)) {
        return Err(AddressError::NotAllowed);
    }
    Ok(Pinned {
        host,
        addrs: ips
            .into_iter()
            .map(|ip| SocketAddr::new(ip, port))
            .collect(),
    })
}
